//! Run the HTTP test server standalone (demos, manual testing):
//!
//!   cargo run -p loadr-testserver --example serve [-- <port>]

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = loadr_testserver::HttpTestServer::spawn().await?;
    println!("test server listening on {}", server.base_url());
    println!("endpoints: / /json /xml /html /echo /status/<n> /delay/<ms> /cookies /gzip /redirect/<n> /login /large/<kb> /headers /counter");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
