use super::mobile::{decode_binary_frame, encode_binary_frame};
use super::{TransportFactory, TransportReader, TransportWriter};
use crate::models::{Request, Response};
use crate::errors::{ClientResult, Error};

use async_trait::async_trait;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use log::{trace, warn};
use yawc::{FrameView, OpCode, WebSocket};

pub struct WebTransport {
    ws: WebSocket,
}

impl WebTransport {
    pub fn new(ws: WebSocket) -> Self {
        Self { ws }
    }
}

impl TransportFactory for WebTransport {
    type Reader = WebReader;
    type Writer = WebWriter;

    fn split(self) -> (Self::Writer, Self::Reader) {
        let (writer, reader) = self.ws.split();
        (WebWriter { writer }, WebReader { reader })
    }
}

pub struct WebWriter {
    writer: SplitSink<WebSocket, FrameView>,
}

#[async_trait]
impl TransportWriter for WebWriter {
    async fn send(&mut self, request: Request) -> ClientResult<()> {
        let buffer = encode_binary_frame(&request)?;

        trace!("WEB Send binary (seq: {}, bytes: {})", request.seq, buffer.len());

        self.writer
            .send(FrameView::binary(buffer.freeze()))
            .await
            .map_err(|e| Error::SendFailed(e.to_string()))?;

        Ok(())
    }
}

pub struct WebReader {
    reader: SplitStream<WebSocket>,
}

#[async_trait]
impl TransportReader for WebReader {
    async fn next_message(&mut self) -> ClientResult<Option<Response>> {
        loop {
            match self.reader.next().await {
                Some(frame) => match frame.opcode {
                    OpCode::Close => return Ok(None),
                    OpCode::Binary => {
                        return decode_binary_frame(&frame.payload).map(Some);
                    }
                    OpCode::Text => {
                        let body_bytes = &frame.payload;
                        match serde_json::from_slice::<Response>(body_bytes) {
                            Ok(resp) => return Ok(Some(resp)),
                            Err(e) => {
                                let body_str = String::from_utf8_lossy(body_bytes);
                                warn!("JSON Error: {} | Body: {}", e, body_str);
                                return Err(Error::ApiResponse(serde_json::json!({"error": e.to_string()})));
                            }
                        }
                    }
                    _ => continue,
                },
                None => return Ok(None),
            }
        }
    }
}
