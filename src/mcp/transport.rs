use std::future::Future;

use rmcp::{
    ErrorData, RoleServer,
    model::{
        ClientJsonRpcMessage, ClientNotification, ClientRequest, ErrorCode, ServerJsonRpcMessage,
    },
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::Transport,
};

pub(crate) struct PreInitTransport<T> {
    inner: T,
    initialized: bool,
}

impl<T> PreInitTransport<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            initialized: false,
        }
    }
}

impl<T> Transport<RoleServer> for PreInitTransport<T>
where
    T: Transport<RoleServer>,
{
    type Error = T::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            let message = self.inner.receive().await?;
            if self.initialized {
                return Some(message);
            }
            match message {
                ClientJsonRpcMessage::Request(request)
                    if matches!(&request.request, ClientRequest::InitializeRequest(_)) =>
                {
                    self.initialized = true;
                    return Some(ClientJsonRpcMessage::Request(request));
                }
                ClientJsonRpcMessage::Request(request)
                    if matches!(&request.request, ClientRequest::CustomRequest(_)) =>
                {
                    let error =
                        ErrorData::new(ErrorCode::METHOD_NOT_FOUND, "Method not found", None);
                    if self
                        .inner
                        .send(ServerJsonRpcMessage::error(error, Some(request.id)))
                        .await
                        .is_err()
                    {
                        return None;
                    }
                }
                ClientJsonRpcMessage::Notification(notification)
                    if matches!(
                        notification.notification,
                        ClientNotification::CustomNotification(_)
                    ) => {}
                other => return Some(other),
            }
        }
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}
