use error_chain::ChainedError;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures_util::{future, pin_mut, SinkExt, StreamExt};
use rs_abieos::{abieos, Abieos};
//use log::*;
//use std::io::prelude::*;
use rust_embed::RustEmbed;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::{Bytes, Message};
use url::Url;
// `error_chain!` can recurse deeply
//#![recursion_limit = "1024"]
//
use errors::{Error, ErrorKind, Result};
#[macro_use]
extern crate lazy_static;
pub mod errors;
pub mod shipper_types;

use crate::shipper_types::{ShipRequests, ShipResultsEx};

#[derive(RustEmbed)]
#[folder = "resources/"]
pub struct ShipAbiFiles;

pub const EOSIO_SYSTEM: &str = "eosio";

pub async fn get_sink_stream(
    server_url: &str,
    mut in_tx: UnboundedReceiver<ShipRequests>,
    out_rx: UnboundedSender<ShipResultsEx>,
) -> Result<()> {
    let r = connect_async(server_url).await?;
    let socket = r.0;
    let (mut sink, mut stream) = socket.split();
    match stream.next().await {
        Some(msg) => {
            let msg_text = msg
                .map_err(|e| {
                    Error::with_chain(e, "get_sink_stream fail")
                })?
                .into_text()
                .map_err(|e| {
                    Error::with_chain(e, "get_sink_stream into_text")
                })?;
            let shipper_abi = Abieos::new();
            shipper_abi.set_abi_json(EOSIO_SYSTEM, msg_text.to_string()).map_err(|e| {
                
                Error::new(ErrorKind::Msg("Error parse ABI".parse().unwrap()), error_chain::State::default())
            })?;

            let out_loop = async {
                loop {
                    let data = stream
                        .next()
                        .await
                        .unwrap()
                        .expect("get_status_request_v0 Response error")
                        .into_data();

                    let r = ShipResultsEx::from_bin(&shipper_abi, &data).unwrap();

                    out_rx.unbounded_send(r).expect("Didn't send");
                }
            };
            let in_loop = async {
                loop {
                    let data: ShipRequests = in_tx.next().await.unwrap();

                    match data {
                        ShipRequests::get_status_request_v0(r) => {
                            let req = r.to_bin(&shipper_abi).unwrap();
                            let msg = Message::Binary(Bytes::from(req));
                            sink.send(msg).await.expect("Didn't send");
                        }
                        ShipRequests::get_blocks_request_v0(br) => {
                            let req = br.to_bin(&shipper_abi).unwrap();
                            let msg = Message::Binary(Bytes::from(req));
                            sink.send(msg).await.expect("Didn't send");
                        }
                        ShipRequests::get_blocks_ack_request_v0(ar) => {
                            let req = ar.to_bin(&shipper_abi).unwrap();
                            let msg = Message::Binary(Bytes::from(req));
                            sink.send(msg).await.expect("Didn't send");
                        }
                        ShipRequests::quit => {
                            eprintln!("QUIT");
                            &sink.close();
                            break;
                        }
                    }
                }
            };
            pin_mut!(in_loop, out_loop);
            future::join(in_loop, out_loop).await;

            Ok(())
        }
        None => {
            Err(ErrorKind::ExpectedABI.into())
        }
    }
}
