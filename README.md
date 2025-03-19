# Rusty Gate - TON Network Proxy

Application that works on user's computer and serves as entry proxy to TON (The Open Network) sites. It works out of the box and only requires to change proxy IP in the browser settings.

## Features

- HTTP/HTTPS proxy for accessing TON network sites
- Special handling for TON domains
- Configurable via command-line arguments or configuration file
- Verbose logging option for debugging

## Installation

### From Source

1. Clone this repository
2. Build the project with Cargo:

```bash
cargo build --release
```

3. The binary will be available in `target/release/rusty-gate`

## Usage

Run the proxy server:

```bash
./rusty-gate
```

By default, the proxy will listen on `127.0.0.1:8080`. You can specify a different address:

```bash
./rusty-gate --bind-address 127.0.0.1:8888
```

### Configuration

You can provide a configuration file:

```bash
./rusty-gate --config-file config.json
```

If the configuration file doesn't exist, a default one will be created.

Example configuration file:

```json
{
  "ton_domains": [
    "ton",
    "t.me"
  ],
  "ton_gateway": "https://gateway.ton.org",
  "verbose_logging": false
}
```

### Logging

Enable verbose logging:

```bash
./rusty-gate --verbose
```

Log to a file instead of stdout:

```bash
./rusty-gate --log-file rusty-gate.log
```

### Browser Configuration

To use the proxy with your browser:

1. Open your browser's network settings
2. Set the HTTP proxy to the address where Rusty Gate is running (default: `127.0.0.1:8080`)
3. Make sure to configure the proxy for both HTTP and HTTPS traffic

#### Chrome

1. Go to Settings → Advanced → System → Open your computer's proxy settings
2. Configure the proxy settings according to your operating system

#### Firefox

1. Go to Options → Network Settings
2. Select "Manual proxy configuration"
3. Enter the proxy address and port
4. Check "Use this proxy server for all protocols"

#### Safari

1. Go to Preferences → Advanced → Proxies → Change Settings
2. Configure the proxy settings for both HTTP and HTTPS

## Development

### Building

```bash
cargo build
```

### Running in Debug Mode

```bash
RUST_LOG=debug cargo run
```

### Running with Verbose Logging

```bash
cargo run -- --verbose
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.