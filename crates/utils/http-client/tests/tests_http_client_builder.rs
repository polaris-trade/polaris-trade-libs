use http::Extensions;
use http_client::{
    HttpClientBuilder,
    middleware::{tracing::TimeTrace, tracing_middleware},
};
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct DummyMiddleware {
    called: Arc<Mutex<bool>>,
}

#[async_trait::async_trait]
impl Middleware for DummyMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut http::Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        *self.called.lock().unwrap() = true;
        next.run(req, extensions).await
    }
}

#[tokio::test]
async fn test_custom_middleware_called() {
    let called = Arc::new(Mutex::new(false));
    let dummy = DummyMiddleware {
        called: called.clone(),
    };

    let client = HttpClientBuilder::new(None).with_middleware(dummy).build();

    let req = client.get("http://example.com").build().unwrap();

    let _ = client.execute(req).await;

    assert!(*called.lock().unwrap(), "Dummy middleware did not run");
}

#[tokio::test]
async fn test_tracing_middleware_executes() {
    let called = Arc::new(Mutex::new(false));

    #[derive(Clone)]
    struct MarkingMiddleware {
        called: Arc<Mutex<bool>>,
    }

    #[async_trait::async_trait]
    impl Middleware for MarkingMiddleware {
        async fn handle(
            &self,
            req: Request,
            ext: &mut Extensions,
            next: Next<'_>,
        ) -> Result<reqwest::Response> {
            *self.called.lock().unwrap() = true;
            next.run(req, ext).await
        }
    }

    let client = HttpClientBuilder::new(None)
        .with_middleware(tracing_middleware())
        .with_middleware(MarkingMiddleware {
            called: called.clone(),
        })
        .build();

    let req = client.get("http://example.com").build().unwrap();
    let _ = client.execute(req).await;

    assert!(
        *called.lock().unwrap(),
        "Middleware after tracing_middleware was not executed"
    );
}

#[tokio::test]
async fn test_timetrace_lifecycle_no_panic() {
    use reqwest_tracing::ReqwestOtelSpanBackend;

    let req = reqwest::Request::new(reqwest::Method::GET, "http://example.com".parse().unwrap());

    let mut ext = Extensions::new();

    let span = TimeTrace::on_request_start(&req, &mut ext);

    assert!(ext.get::<std::time::Instant>().is_some());

    let result: Result<reqwest::Response> = Err(reqwest_middleware::Error::Middleware(
        std::io::Error::other("fake error").into(),
    ));

    TimeTrace::on_request_end(&span, &result, &mut ext);
}
