use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub author: String
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GetMode{
    ALL, 
    ByTitle(String),
    ById(String),
    ByAuthor(String)
}


#[derive(Debug, Serialize, Deserialize)]
pub struct SongRequest{
    pub mode: GetMode,
    pub requester_peer_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SongResponse{
    pub mode: GetMode,
    pub responser_peer_id: String,
    pub requester_peer_id: String,
    pub data: Vec<Song>
}

pub enum EventType {
    Response(SongResponse),
    Input(String),
}