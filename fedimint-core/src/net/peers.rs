use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use async_trait::async_trait;
use fedimint_core::PeerId;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::task::Cancellable;

#[cfg(not(target_family = "wasm"))]
pub mod fake;

/// Owned [`PeerConnections`] trait object type
pub struct PeerConnections<Msg>(Box<dyn IPeerConnections<Msg> + Send + 'static>);

impl<Msg> Deref for PeerConnections<Msg> {
    type Target = dyn IPeerConnections<Msg> + Send + 'static;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl<Msg> DerefMut for PeerConnections<Msg> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}

/// Connection manager that tries to keep connections open to all peers
#[async_trait]
pub trait IPeerConnections<M>
where
    M: Serialize + DeserializeOwned + Unpin + Send,
{
    /// Send message to recipient; block if channel is full.
    async fn send(&mut self, recipient: Recipient, msg: M);

    /// Try to send message to recipient; drop message if channel is full.
    fn try_send(&self, recipient: Recipient, msg: M);

    /// Await receipt of a message; return None if we are shutting down.
    async fn receive(&mut self) -> Option<(PeerId, M)>;

    /// Converts the struct to a `PeerConnection` trait object
    fn into_dyn(self) -> PeerConnections<M>
    where
        Self: Sized + Send + 'static,
    {
        PeerConnections(Box::new(self))
    }
}

/// This enum defines the intended recipient of a p2p message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipient {
    Everyone,
    Peer(PeerId),
}

/// Owned [`MuxPeerConnections`] trait object type
#[derive(Clone)]
pub struct MuxPeerConnections<MuxKey, Msg>(
    Arc<dyn IMuxPeerConnections<MuxKey, Msg> + Send + Sync + Unpin + 'static>,
);

impl<MuxKey, Msg> Deref for MuxPeerConnections<MuxKey, Msg> {
    type Target = dyn IMuxPeerConnections<MuxKey, Msg> + Send + Sync + Unpin + 'static;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

#[async_trait]
/// Like [`IPeerConnections`] but with an ability to handle multiple
/// destinations (like modules) per each peer-connection.
///
/// Notably, unlike [`IPeerConnections`] implementations need to be thread-safe,
/// as the primary intended use should support multiple threads using
/// multiplexed channel at the same time.
pub trait IMuxPeerConnections<MuxKey, Msg>
where
    Msg: Serialize + DeserializeOwned + Unpin + Send,
    MuxKey: Serialize + DeserializeOwned + Unpin + Send,
{
    /// Send a message to a specific destination at specific peer.
    async fn send(&self, peers: &[PeerId], mux_key: MuxKey, msg: Msg) -> Cancellable<()>;

    /// Await receipt of a message from any connected peer.
    async fn receive(&self, mux_key: MuxKey) -> Cancellable<(PeerId, Msg)>;

    /// Converts the struct to a `PeerConnection` trait object
    fn into_dyn(self) -> MuxPeerConnections<MuxKey, Msg>
    where
        Self: Sized + Send + Sync + Unpin + 'static,
    {
        MuxPeerConnections(Arc::new(self))
    }
}
