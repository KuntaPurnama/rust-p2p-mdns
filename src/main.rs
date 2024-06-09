use libp2p::{
    core::upgrade, floodsub::Floodsub, futures::StreamExt, mdns::Mdns, mplex, noise::{Keypair, NoiseConfig, X25519Spec},
    request_response::{ProtocolSupport, RequestResponse, RequestResponseConfig}, swarm::{Swarm, SwarmBuilder},
    tcp::TokioTcpConfig, PeerId, Transport
};
use log::info;
use std::str::FromStr;
use tokio::{io::AsyncBufReadExt, sync::mpsc};
use uuid::Uuid;

mod network;
mod song;
mod utils;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    info!("Peer Id: {}", utils::PEER_ID.clone());
    let (response_sender, mut response_receiver) = mpsc::unbounded_channel();

    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&utils::KEYS)
        .expect("can create auth keys");

    //create transport
    let transport = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();


    let request_response = RequestResponse::new(network::SongCodec, vec![(network::SongProtocol, ProtocolSupport::Full)], RequestResponseConfig::default());

    //initiate network behaviour   
    let mut network_behaviour = network::SongNetworkBehaviour{
        floodsub: Floodsub::new(utils::PEER_ID.clone()),
        mdns: Mdns::new(Default::default()).await.expect("can create mdns"),
        request_response: request_response,
        response_sender
    };

    network_behaviour.floodsub.subscribe(utils::TOPIC.clone());

    //create swarm config
    let mut swarm = SwarmBuilder::new(transport, network_behaviour,  utils::PEER_ID.clone())
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    //start swarm 
    // Similar to starting, for example, a TCP server, we simply call listen_on with a local IP, 
    // letting the OS decide the port for us. This will start the Swarm with all of our setup, but we haven’t actually defined any logic yet.
    Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/0".parse().expect("can get local socker")).expect("swarm is started");

    //Handling user input from console. Input form is more like a command line we used to see
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    //create loop to handle incoming input or response
    loop{
        let evt = {
            tokio::select! {
                line = stdin.next_line() => Some(song::EventType::Input((line.expect("can get line")).expect("can read line"))),
                event = swarm.next() => {
                    // info!("Unhandled Swarm Event: {:?}", event);
                    None  
                },
                response = response_receiver.recv() => Some(song::EventType::Response(response.expect("response exists"))),
            }
        };

        if let Some(event) = evt {
            match event {
                song::EventType::Input(input) => {
                    match input.as_str() {
                        "ls p" => handle_list_peers(&mut swarm).await,
                        cmd if cmd.starts_with("ls s") => handle_list_of_songs(&mut swarm, &cmd).await,
                        cmd if cmd.starts_with("create s") => handle_create_song(&mut swarm, &cmd).await,
                        _ => {info!("Invalid command line")}
                    };
                },
                song::EventType::Response(responses) => {
                    match PeerId::from_str(responses.requester_peer_id.as_str()) {
                        Ok(peer_id) => {
                            // Successfully parsed PeerId
                            // Handle peer_id here
                            swarm.behaviour_mut().request_response.send_request(&peer_id, responses);
                        }
                        Err(err) => {
                            // Parsing failed
                            // Handle the error here
                            info!("Error parsing PeerId: {:?}", err);
                        }
                    };
                }
            }
        }
    }
}

async fn handle_list_of_songs(swarm: &mut Swarm<network::SongNetworkBehaviour>, cmd: &str){
    if let Some(rest) = cmd.strip_prefix("ls s "){
        info!("rest {}", rest);
        match rest {
            "all" => {
                let request = song::SongRequest{
                    mode: song::GetMode::ALL,
                    requester_peer_id: utils::PEER_ID.clone().to_string()
                };
                let request_string = serde_json::to_string(&request).unwrap();
                swarm.behaviour_mut().floodsub.publish(utils::TOPIC.clone(), request_string.as_bytes());
            }
            command if command.starts_with("id") => {
                let id = command.strip_prefix("id ").unwrap();
                let request = song::SongRequest{
                    mode: song::GetMode::ById(String::from(id)),
                    requester_peer_id: utils::PEER_ID.clone().to_string()
                };
                let request_string = serde_json::to_string(&request).unwrap();
                swarm.behaviour_mut().floodsub.publish(utils::TOPIC.clone(), request_string.as_bytes());
            }
            command if command.starts_with("author") => {
                let author = command.strip_prefix("author ").unwrap();
                let request = song::SongRequest{
                    mode: song::GetMode::ByAuthor(String::from(author)),
                    requester_peer_id: utils::PEER_ID.clone().to_string()
                };
                let request_string = serde_json::to_string(&request).unwrap();
                swarm.behaviour_mut().floodsub.publish(utils::TOPIC.clone(), request_string.as_bytes());
            }
            command if command.starts_with("title") => {
                let title = command.strip_prefix("title ").unwrap();
                let request = song::SongRequest{
                    mode: song::GetMode::ByTitle(String::from(title)),
                    requester_peer_id: utils::PEER_ID.clone().to_string()
                };
                let request_string = serde_json::to_string(&request).unwrap();
                swarm.behaviour_mut().floodsub.publish(utils::TOPIC.clone(), request_string.as_bytes());
            }
            _ => {info!("Invalid get command")}
        }
    }
}

async fn handle_list_peers(swarm: &mut Swarm<network::SongNetworkBehaviour>){
    info!("Discovered Peers");
    let nodes = swarm.behaviour_mut().mdns.discovered_nodes();
    
    nodes.into_iter().for_each(|p| info!("{}", p));
}

async fn handle_create_song(swarm: &mut Swarm<network::SongNetworkBehaviour>, cmd :&str){
    if let Some(rest) = cmd.strip_prefix("create s ") {
        let words: Vec<&str> = rest.split("|").collect();
        if words.len() < 2{
            info!("Too few arguments - Format: title|author")
        }else if words.len() > 2 {
            info!("Too many arguments - Format: title|author");
        }else{
            let title = words[0];
            let author = words[1];

            if let Err(e) = create_new_song(title, author).await{
                info!("error when create new song {}", e);
            }
        }
    }
}

async fn create_new_song(title: &str, author: &str) -> Result<()>{
    let mut songs = utils::read_local_songs().await?;
    let new_song = song::Song{
        id: Uuid::new_v4().to_string(),
        title: String::from(title),
        author: String::from(author)
    };
    songs.push(new_song);
    let _ = utils::write_songs_to_local_storage(songs).await?;
    info!("Success create new song");
    Ok(())
}