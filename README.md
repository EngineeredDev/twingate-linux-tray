# Twingate Linux Tray

A system tray application for managing Twingate VPN connections on Linux. This application provides an intuitive graphical interface for controlling the Twingate service, viewing network resources, and handling authentication directly from your system tray.

## Features

- **System Tray Integration**: Native Linux system tray icon with context menu
- **Service Management**: Start, stop, and monitor Twingate service status
- **Resource Access**: View and connect to available network resources
- **Authentication Handling**: Automated browser-based authentication flow
- **Desktop Notifications**: Get notified when authentication is required
- **Clipboard Integration**: Copy resource addresses with one click
- **Real-time Status**: Live updates of connection status and available resources

## Prerequisites

> **⚠️ Important**: You must have the Twingate CLI installed before using this application.
> 
> Install the latest Twingate CLI following the instructions at:  
> **https://www.twingate.com/docs/linux**

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

### System Tray Menu

Once running, the application appears as an icon in your system tray. Right-click the icon to access:

- **User Information**: View your current Twingate user details (when connected)
- **Network Resources**: List of available resources with options to:
  - Copy resource addresses to clipboard
  - Authenticate to restricted resources
- **Service Control**: Start or stop the Twingate service
- **Status Information**: Current connection status and service state

### Authentication

When accessing resources that require authentication:
1. Click "Authenticate" next to the resource in the tray menu
2. A desktop notification will appear with the authentication URL
3. Your default browser will automatically open to the authentication page
4. Complete the authentication in your browser
5. The tray menu will update to show the authenticated status

## Service States

The application displays different states based on your Twingate service status:

- **Not Running**: Service is stopped - click to start
- **Starting**: Service is initializing
- **Connecting**: Service is establishing connection
- **Connected**: Service is active and ready
- **Auth Required**: Authentication needed - follow the prompts

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

## Troubleshooting

### Application doesn't start
- Ensure the Twingate CLI is installed and accessible in your PATH
- Check system tray support is enabled in your desktop environment

### Authentication issues
- Verify your default browser is configured correctly
- Check that popup blockers aren't preventing the authentication page from opening
- Ensure you have an active internet connection

### Service control problems
- Confirm the Twingate service is properly installed
- Check that your user has appropriate permissions to control the service

## Support

For issues related to:
- **Twingate Linux Tray**: Open an issue on this repository
- **Twingate CLI or Service**: Visit [Twingate Support](https://www.twingate.com/docs/linux)

## License

This project is licensed under the terms specified in the LICENSE file.