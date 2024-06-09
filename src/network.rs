use libp2p::{
    floodsub::{Floodsub, FloodsubEvent}, mdns::{Mdns, MdnsEvent},
    request_response::{ProtocolName, RequestResponse, RequestResponseCodec, RequestResponseEvent, RequestResponseMessage},
    swarm::NetworkBehaviourEventProcess, NetworkBehaviour
};
use log::{error, info};
use tokio::sync::mpsc;
use std::io;
use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::song;
use crate::utils;

#[derive(NetworkBehaviour)]
pub struct SongNetworkBehaviour{
    pub floodsub: Floodsub,
    pub mdns: Mdns,
    pub request_response: RequestResponse<SongCodec>,
    #[behaviour(ignore)]
    pub response_sender: mpsc::UnboundedSender<song::SongResponse>,
}

#[derive(Debug, Clone)]
pub struct SongProtocol;

impl ProtocolName for SongProtocol {
    fn protocol_name(&self) -> &[u8] {
        b"/song/1.0.0"
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for SongNetworkBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, addr) in list {
                    info!("Discovered new Peer : {:?}, addr: {:?}", peer, addr);
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, addr) in list {
                    if !self.mdns.has_node(&peer){
                        info!("Expired Peer : {:?}, addr: {:?}", peer, addr);
                        self.floodsub.remove_node_from_partial_view(&peer);
                        self.request_response.remove_address(&peer, &addr);
                    }
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for SongNetworkBehaviour{
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(msg) => {
                if let Ok(req) = serde_json::from_slice::<song:: SongRequest>(&msg.data){
                    info!("Receive request to get songs from {:?}", msg);
                    get_songs(req, self.response_sender.clone());
                }
            },
            FloodsubEvent::Subscribed { peer_id, topic } => {
                // info!("Peer {} subscribe topic: {:?}", peer_id, topic);
            },
            FloodsubEvent::Unsubscribed { peer_id, topic } => {
                // info!("Peer {} unsubscribe topic: {:?}", peer_id, topic);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<RequestResponseEvent<song::SongResponse, String>> for SongNetworkBehaviour {
    fn inject_event(&mut self, event: RequestResponseEvent<song::SongResponse, String>) {
        match event {
            RequestResponseEvent::Message { peer, message } => {
                match message {
                    RequestResponseMessage::Request { request_id, request, channel } => {
                        // info!("Got message requestId : {:?}, response : {:?}, channel: {:?}", request_id, request, channel);
                        self.request_response.send_response(channel, String::from("OK")).unwrap();
                        request.data.into_iter().for_each(|p| info!("{:?}", p));
                    }
                    RequestResponseMessage::Response { request_id, response } => {
                        // info!("Receive Response requestId : {:?}, response : {:?}", request_id, response)
                    }
                }
            }   
            RequestResponseEvent::InboundFailure { peer, request_id, error } => {
                info!("Error when receive a message with peerId : {:?}, requestId: {:?}, error: {:?}", peer, request_id, error);
            }
            RequestResponseEvent::OutboundFailure { peer, request_id, error } => {
                info!("Error when get send a message with peerId : {:?}, requestId: {:?}, error: {:?}", peer, request_id, error);
            }
            RequestResponseEvent::ResponseSent { peer, request_id } => {
                // info!("Success send a message with peerId : {:?}, requestId: {:?}", peer, request_id);
            }
            
        }
    }
}

// Implement the codec for the custom protocol
#[derive(Clone)]
pub struct SongCodec;

#[async_trait]
impl RequestResponseCodec for SongCodec {
    type Protocol = SongProtocol;
    type Request = song::SongResponse;
    type Response = String;

    async fn read_request<T>(&mut self, _: &SongProtocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        // println!("read_request triggered");
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        let request = serde_json::from_slice(&buf)?;
        Ok(request)
    }

    async fn read_response<T>(&mut self, _: &SongProtocol, io: &mut T) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        // println!("read_response triggered");
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        let response = serde_json::from_slice(&buf)?;
        Ok(response)
    }

    async fn write_request<T>(
        &mut self,
        _: &SongProtocol,
        io: &mut T,
        request: song::SongResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // println!("write_request triggered");
        let buf = serde_json::to_vec(&request)?;
        io.write_all(&buf).await?;
        io.close().await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &SongProtocol,
        io: &mut T,
        response: String,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // println!("write_response triggered");
        let buf = serde_json::to_vec(&response)?;
        io.write_all(&buf).await?;
        io.close().await?;
        Ok(())
    }
}

fn get_songs(req: song::SongRequest, sender: mpsc::UnboundedSender<song::SongResponse>) {
    tokio::spawn(async move {
        let mut songs = utils::read_local_songs().await.unwrap();

        match req.mode {
            song::GetMode::ALL => {
                info!("Get all songs with request: {:?}", req);
            },
            song::GetMode::ByAuthor(ref author) => {
                info!("Get song by author with request: {:?}", req);
                songs = songs.into_iter().filter(|data| data.author == *author).collect();
            }
            song::GetMode::ById(ref id) => {
                info!("Get song by id with request: {:?}", req);
                songs = songs.into_iter().filter(|data| data.id == *id).collect();
            }  
            song::GetMode::ByTitle(ref title) => {
                info!("Get song by title with request: {:?}", req);
                songs = songs.into_iter().filter(|data| data.title == *title).collect();
            }
            _ => {
                info!("Unexpected mode : {:?}", req.mode);
                songs = Vec::new();
            }
        }

        let song_response = song::SongResponse {
            mode: song::GetMode::ALL,
            responser_peer_id: utils::PEER_ID.to_string().clone(),
            requester_peer_id: req.requester_peer_id.clone(),
            data: songs
        };

        if let Err(e) = sender.send(song_response) {
            error!("error sending response via channel, {}", e);
        }
    });
}

