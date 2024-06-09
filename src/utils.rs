use tokio::fs;
use once_cell::sync::Lazy;

use libp2p::{
    floodsub::Topic,identity, PeerId
};

use crate::song;

const STORAGE_FILE_PATH: &str = "./songs.json";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

pub static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
pub static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("songs"));

pub async fn write_songs_to_local_storage(songs: Vec<song::Song>) -> Result<()>{
    let bytes = serde_json::to_string(&songs).unwrap();
    let _ = fs::write(STORAGE_FILE_PATH, &bytes).await;
    Ok(())
}

pub async fn read_local_songs() -> Result<Vec<song::Song>>{
    let content = fs::read(STORAGE_FILE_PATH).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}