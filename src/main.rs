mod config;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use config::Config;
use futures::future::try_join;
use http::{Request, Response, StatusCode, Uri, Method};
use hyper::service::{make_service_fn, service_fn};
use hyper::upgrade::Upgraded;
use hyper::{Body, Client, Server};
use hyper_tls::HttpsConnector;
use log::{error, info, debug, warn};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Command line arguments for the proxy server
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Address to bind the proxy server to
    #[clap(short, long, default_value = "127.0.0.1:8080")]
    bind_address: String,

    /// Path to the configuration file
    #[clap(short, long)]
    config_file: Option<PathBuf>,

    /// Enable verbose logging
    #[clap(short, long)]
    verbose: bool,
    
    /// Path to log file (if not specified, logs to stdout)
    #[clap(short, long)]
    log_file: Option<PathBuf>,
}

/// Application state shared across request handlers
struct AppState {
    client: Client<HttpsConnector<hyper::client::HttpConnector>>,
    config: Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize logger
    if let Some(log_file) = &args.log_file {
        // Setup file logger
        let log_file = std::fs::File::create(log_file)
            .context("Failed to create log file")?;
        
        let env = env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
        let mut builder = env_logger::Builder::from_env(env);
        builder.target(env_logger::Target::Pipe(Box::new(log_file)));
        builder.init();
    } else {
        // Setup stdout logger
        env_logger::init_from_env(
            env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
        );
    }

    let addr: SocketAddr = args
        .bind_address
        .parse()
        .context("Failed to parse bind address")?;

    // Load configuration
    let mut config = Config::default();
    if let Some(config_path) = &args.config_file {
        match Config::from_file(config_path) {
            Ok(loaded_config) => {
                info!("Loaded configuration from {}", config_path.display());
                config = loaded_config;
            }
            Err(e) => {
                error!("Failed to load configuration: {}", e);
                info!("Using default configuration");
                
                // Save default configuration for reference
                if let Err(e) = config.to_file(config_path) {
                    error!("Failed to save default configuration: {}", e);
                } else {
                    info!("Saved default configuration to {}", config_path.display());
                }
            }
        }
    }

    // Override config with command line arguments
    if args.verbose {
        config.verbose_logging = true;
    }

    // Create a client with TLS support for HTTPS requests
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, Body>(https);

    // Create application state
    let state = Arc::new(AppState { client, config });

    // Create a service function that will handle incoming requests
    let make_svc = make_service_fn(move |_conn| {
        let state = Arc::clone(&state);
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let state = Arc::clone(&state);
                async move { handle_request(state, req).await }
            }))
        }
    });

    // Create and start the server
    let server = Server::bind(&addr).serve(make_svc);
    info!("Proxy server listening on {}", addr);
    info!("Configure your browser to use this proxy for HTTP and HTTPS traffic");

    // Run the server
    server.await.context("Server error")?;

    Ok(())
}

/// Main request handler that dispatches to appropriate handlers based on request method
async fn handle_request(
    state: Arc<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, Infallible> {
    if state.config.verbose_logging {
        debug!("Received request: {} {}", req.method(), req.uri());
    }
    
    // Handle CONNECT method differently for HTTPS tunneling
    if req.method() == Method::CONNECT {
        match handle_connect(req).await {
            Ok(response) => Ok(response),
            Err(e) => {
                error!("CONNECT error: {}", e);
                let mut response = Response::new(Body::from(format!("CONNECT error: {}", e)));
                *response.status_mut() = StatusCode::BAD_GATEWAY;
                Ok(response)
            }
        }
    } else {
        // Handle regular HTTP requests
        match proxy(state, req).await {
            Ok(response) => Ok(response),
            Err(e) => {
                error!("Proxy error: {}", e);
                let mut response = Response::new(Body::from(format!("Proxy error: {}", e)));
                *response.status_mut() = StatusCode::BAD_GATEWAY;
                Ok(response)
            }
        }
    }
}

/// Handle HTTPS CONNECT requests by establishing a tunnel
async fn handle_connect(req: Request<Body>) -> Result<Response<Body>> {
    // Extract the target address from the request URI
    let uri = req.uri();
    let addr = uri.authority()
        .ok_or_else(|| anyhow!("CONNECT request missing authority"))?
        .to_string();
    
    info!("CONNECT request to {}", addr);
    
    // Create a response that will be upgraded
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::OK;
    
    // Spawn a task to handle the tunnel after the response is sent
    tokio::task::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                if let Err(e) = tunnel(upgraded, addr).await {
                    error!("Tunnel error: {}", e);
                }
            }
            Err(e) => {
                error!("Upgrade error: {}", e);
            }
        }
    });
    
    Ok(response)
}

/// Create a tunnel between the client and the target server
async fn tunnel(mut upgraded: Upgraded, addr: String) -> Result<()> {
    // Connect to the target server
    let mut server = TcpStream::connect(addr).await?;
    
    // Create bidirectional streams
    let (mut client_read, mut client_write) = tokio::io::split(upgraded);
    let (mut server_read, mut server_write) = server.split();
    
    // Copy data in both directions
    let client_to_server = async {
        let mut buffer = [0; 8192];
        loop {
            let n = client_read.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            server_write.write_all(&buffer[..n]).await?;
        }
        server_write.shutdown().await?;
        Ok::<_, anyhow::Error>(())
    };
    
    let server_to_client = async {
        let mut buffer = [0; 8192];
        loop {
            let n = server_read.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            client_write.write_all(&buffer[..n]).await?;
        }
        client_write.shutdown().await?;
        Ok::<_, anyhow::Error>(())
    };
    
    // Run both directions concurrently
    let (client_result, server_result) = tokio::join!(client_to_server, server_to_client);
    client_result?;
    server_result?;
    
    Ok(())
}

/// Proxy function that forwards requests to the target server
async fn proxy(
    state: Arc<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, Infallible> {
    match proxy_internal(state, req).await {
        Ok(response) => Ok(response),
        Err(e) => {
            error!("Proxy error: {}", e);
            let mut response = Response::new(Body::from(format!("Proxy error: {}", e)));
            *response.status_mut() = StatusCode::BAD_GATEWAY;
            Ok(response)
        }
    }
}

/// Internal proxy implementation
async fn proxy_internal(
    state: Arc<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>> {
    let uri = req.uri().clone();
    
    if state.config.verbose_logging {
        debug!(
            "Received request: {} {} {}",
            req.method(),
            uri,
            req.version()
        );
    }

    // Check if this is a TON domain
    let host = uri.host().unwrap_or("");
    let is_ton_domain = state.config.is_ton_domain(host);
    
    if is_ton_domain {
        info!("Handling TON domain request: {}", uri);
        
        // Modify the request to go through the TON gateway
        let new_uri = rewrite_ton_uri(&uri, &state.config.ton_gateway)?;
        
        if state.config.verbose_logging {
            debug!("Rewritten URI: {}", new_uri);
        }
        
        // Create a new request with the rewritten URI
        let (mut parts, body) = req.into_parts();
        parts.uri = new_uri;
        let new_req = Request::from_parts(parts, body);
        
        // Forward the request to the TON gateway
        match state.client.request(new_req).await {
            Ok(response) => {
                info!("TON gateway response status: {}", response.status());
                Ok(response)
            }
            Err(e) => {
                error!("TON gateway request failed: {}", e);
                let mut response = Response::new(Body::from(
                    format!("Failed to connect to TON gateway: {}", e)
                ));
                *response.status_mut() = StatusCode::BAD_GATEWAY;
                Ok(response)
            }
        }
    } else {
        // Regular proxy handling for non-TON domains
        info!("Proxying regular request: {}", uri);
        
        // Create a new request with the same parts
        let (parts, body) = req.into_parts();
        let new_req = Request::from_parts(parts, body);
        
        // Forward the request to the target server
        match state.client.request(new_req).await {
            Ok(response) => {
                info!("Response status: {}", response.status());
                Ok(response)
            }
            Err(e) => {
                error!("Request failed: {}", e);
                let mut response = Response::new(Body::from(
                    format!("Failed to connect to target server: {}", e)
                ));
                *response.status_mut() = StatusCode::BAD_GATEWAY;
                Ok(response)
            }
        }
    }
}

/// Rewrite a URI to go through the TON gateway
fn rewrite_ton_uri(uri: &Uri, gateway: &str) -> Result<Uri> {
    let host = uri.host().unwrap_or("");
    let path = uri.path();
    let query = uri.query().map(|q| format!("?{}", q)).unwrap_or_default();
    
    // Construct the new URI
    let new_uri_str = format!("{}/{}{}{}",
        gateway.trim_end_matches('/'),
        host,
        path,
        query
    );
    
    let new_uri = new_uri_str.parse::<Uri>()
        .context(format!("Failed to parse rewritten URI: {}", new_uri_str))?;
        
    Ok(new_uri)
}