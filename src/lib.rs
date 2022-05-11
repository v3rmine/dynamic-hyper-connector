use std::sync::Arc;

use futures_util::{future::BoxFuture, AsyncRead, AsyncWrite};
use hyper::{client::HttpConnector, service::Service, Body, Client, Uri};
use hyperlocal::UnixConnector;

trait DynResponse: Send + Sync {}
impl<T> DynResponse for T
where
    T: AsyncRead + AsyncWrite,
    T: Send + Sync,
{
}
type ArcDynResponse = Arc<dyn DynResponse>;

trait DynError: Send + Sync {}
impl<T> DynError for T
where
    T: std::error::Error,
    T: Send + Sync,
{
}
type ArcDynError = Arc<dyn DynError>;

type DynamicConnector = Arc<
    dyn Service<
        Uri,
        Response = ArcDynResponse,
        Error = ArcDynError,
        Future = BoxFuture<'static, Result<ArcDynResponse, ArcDynError>>,
    >,
>;

fn new_connector(url: &str) -> DynamicConnector {
    if url.starts_with("unix://") {
        Arc::new(UnixConnector::default())
    } else {
        Arc::new(HttpConnector::new())
    }
}

fn generic_client(url: &str) -> Client<DynamicConnector, Body> {
    Client::builder().build(new_connector(url))
}

// Individual Builds
fn build_unix() -> Client<UnixConnector, Body> {
    Client::builder().build(UnixConnector::default())
}

fn build_http() -> Client<HttpConnector, Body> {
    Client::builder().build(HttpConnector::new())
}
