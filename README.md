# Twingate Linux Tray

A system tray application for Twingate on Linux. This provides an system tray for controlling the Twingate service.

## Prerequisites

> **⚠️ Important**: You must have the Twingate CLI installed before using this application.
> 
>   Install the latest Twingate CLI following the instructions at: **https://www.twingate.com/docs/linux**
>
> ⚠️ Currently, first-time setup is **NOT** supported. You must setup via the CLI with `sudo twingate setup` as explained in the docs.

## Installation

Download the appropriate package for your Linux distribution from the [Releases](../../releases) page:

### Debian/Ubuntu (.deb)
```bash
# Download the .deb file from releases, then:
sudo dpkg -i twingate-linux-tray_*.deb
```

### Red Hat/Fedora/CentOS (.rpm)
```bash
# Download the .rpm file from releases, then:
sudo rpm -i twingate-linux-tray-*.rpm
```

### Universal Linux (AppImage)
```bash
# Download the .AppImage file from releases, then:
chmod +x twingate-linux-tray-*.AppImage
./twingate-linux-tray-*.AppImage
```

## Usage

### Starting the Application

After installation, you can start the application from:
- **Applications menu**: Look for "Twingate Linux Tray"
- **Command line**: Run `twingate-linux-tray`
- **Autostart**: The application can be configured to start automatically with your desktop session

## Development

This application is built with:
- **Backend**: Rust with Tauri framework
- **Frontend**: React with TypeScript
- **Build System**: Vite for frontend, Tauri for packaging

### Building from Source

```bash
# Install dependencies
npm install

# Development mode with hot reload
npm run tauri dev

# Build production packages
npm run tauri build
```

## License

This project is licensed under the terms specified in the LICENSE file.
