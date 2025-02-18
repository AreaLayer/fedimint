use std::io::Cursor;

use anyhow::Context;
use async_trait::async_trait;
use bytes::Bytes;
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::module::registry::ModuleDecoderRegistry;
use futures::{SinkExt, StreamExt};
use iroh::endpoint::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_rustls::TlsStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub type DynP2PConnection<M> = Box<dyn IP2PConnection<M>>;

#[async_trait]
pub trait IP2PConnection<M>: Send + 'static {
    async fn send(&mut self, message: M) -> anyhow::Result<()>;

    async fn receive(&mut self) -> anyhow::Result<M>;

    fn into_dyn(self) -> DynP2PConnection<M>
    where
        Self: Sized,
    {
        Box::new(self)
    }
}

#[async_trait]
impl<M> IP2PConnection<M> for Framed<TlsStream<TcpStream>, LengthDelimitedCodec>
where
    M: Serialize + DeserializeOwned + Send + 'static,
{
    async fn send(&mut self, message: M) -> anyhow::Result<()> {
        let mut bytes = Vec::new();

        bincode::serialize_into(&mut bytes, &message)?;

        SinkExt::send(self, Bytes::from_owner(bytes)).await?;

        Ok(())
    }

    async fn receive(&mut self) -> anyhow::Result<M> {
        Ok(bincode::deserialize_from(Cursor::new(
            &self.next().await.context("Framed stream is closed")??,
        ))?)
    }
}

#[async_trait]
impl<M> IP2PConnection<M> for Connection
where
    M: Encodable + Decodable + Send + 'static,
{
    async fn send(&mut self, message: M) -> anyhow::Result<()> {
        let mut sink = self.open_uni().await?;

        sink.write_all(&message.consensus_encode_to_vec()).await?;

        sink.finish()?;

        Ok(())
    }

    async fn receive(&mut self) -> anyhow::Result<M> {
        let bytes = self.accept_uni().await?.read_to_end(1_000_000_000).await?;

        Ok(Decodable::consensus_decode_whole(
            &bytes,
            &ModuleDecoderRegistry::default(),
        )?)
    }
}
