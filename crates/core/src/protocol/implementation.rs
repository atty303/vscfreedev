//! Concrete protocol implementations using the unified abstraction

use super::{MessageCodec, Protocol, Request, Response};
use crate::error::Result;
use crate::message_channel::MessageChannel;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

/// Generic protocol implementation that works with any transport and codec
pub struct GenericProtocol<T, Req, Resp, Codec>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
    Req: Request,
    Resp: Response,
    Codec: MessageCodec,
{
    channel: MessageChannel<T>,
    codec: Codec,
    _phantom: std::marker::PhantomData<(Req, Resp)>,
}

impl<T, Req, Resp, Codec> GenericProtocol<T, Req, Resp, Codec>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
    Req: Request,
    Resp: Response,
    Codec: MessageCodec,
{
    pub fn new(transport: T, codec: Codec) -> Self {
        Self {
            channel: MessageChannel::new_with_stream(transport),
            codec,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<T, Req, Resp, Codec> Protocol<Req, Resp> for GenericProtocol<T, Req, Resp, Codec>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    Req: Request + 'static,
    Resp: Response + 'static,
    Codec: MessageCodec + Send + Sync + 'static,
{
    async fn send_request(&mut self, request: Req) -> Result<Resp> {
        // Encode the request
        let request_bytes = self.codec.encode(&request)?;

        // Send via channel
        self.channel.send(request_bytes).await?;

        // Receive response
        let response_bytes = self.channel.receive().await?;

        // Decode the response
        let response = self.codec.decode(response_bytes)?;

        Ok(response)
    }

    async fn receive_request(&mut self) -> Result<Req> {
        // Receive request bytes
        let request_bytes = self.channel.receive().await?;

        // Decode the request
        let request = self.codec.decode(request_bytes)?;

        Ok(request)
    }

    async fn send_response(&mut self, response: Resp) -> Result<()> {
        // Encode the response
        let response_bytes = self.codec.encode(&response)?;

        // Send via channel
        self.channel.send(response_bytes).await?;

        Ok(())
    }

    fn name(&self) -> &'static str {
        "generic"
    }
}

/// Type aliases for specific protocol implementations
pub type SimpleProtocol<T> = GenericProtocol<
    T,
    super::simple::SimpleRequest,
    super::simple::SimpleResponse,
    super::codec::JsonCodec,
>;
pub type DaemonProtocol<T> = GenericProtocol<
    T,
    super::daemon::DaemonRequest,
    super::daemon::DaemonResponse,
    super::codec::JsonCodec,
>;

/// Protocol factory for simple protocol
pub struct SimpleProtocolFactory;

impl SimpleProtocolFactory {
    pub fn create<T>(transport: T) -> SimpleProtocol<T>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        GenericProtocol::new(transport, super::codec::JsonCodec::new())
    }
}

/// Protocol factory for daemon protocol
pub struct DaemonProtocolFactory;

impl DaemonProtocolFactory {
    pub fn create<T>(transport: T) -> DaemonProtocol<T>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        GenericProtocol::new(transport, super::codec::JsonCodec::new())
    }
}
