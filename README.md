qtilerugo

qtilerugo is an integrated window management ecosystem for Qtile. It combines three custom components to extend your desktop environment on X11:

    xcb_wm_bridge: A Rust-based command bridge that interacts with the X server using x11rb and executes window management commands from a configurable TOML file.
    Rust Renderer: A custom Vulkan-based window renderer written in Rust using vulkano and winit that spawns custom-rendered windows or external applications.
    GoBar: A Go-based taskbar and system tray built with Fyne that displays system stats, provides a start menu, and integrates with X11 (using xgb).

Features

    Unified Command Interface:
    Send JSON-encoded commands over Unix sockets to control window management actions.

    Customizable and Modular:
    Each component (bridge, renderer, and taskbar) is self-contained and can be developed or replaced independently while integrating seamlessly.

    Advanced Rendering Capabilities:
    Leverage Vulkan via the Rust renderer for custom windows and dynamic interfaces.

    Integration with Qtile:
    Easily bind keys in your Qtile configuration to send commands to qtilerugo, extending Qtile’s functionality.

Components Overview
xcb_wm_bridge

    Purpose:
    Acts as a command relay between Qtile and X11. It listens on a Unix socket (e.g., /tmp/x11rb_wm.sock) for JSON commands and executes corresponding window management actions (like focusing windows or spawning terminals).

    Configuration:
    Commands are mapped via a wm_config.toml file for easy customization.

Rust Renderer

    Purpose:
    Uses Vulkan to create custom-rendered windows. It listens on a Unix socket (e.g., /tmp/rust_qtile_helper.sock) for JSON commands such as SpawnWindow and SpawnStatusBar.

    Functionality:
    Spawns a Vulkan window or launches external applications (like the GoBar) based on incoming commands.

GoBar

    Purpose:
    A lightweight, customizable taskbar and system tray application written in Go.
    Features:
        Displays time, CPU usage, and network statistics.
        Provides a "Start Menu" by scanning installed .desktop files.
        Integrates with X11 by setting dock properties to reserve screen space.
        Uses systray functionality to add interactive tray icons for launching common applications.

Contributing

Contributions are welcome! If you have ideas for improvements, new features, or bug fixes, please open an issue or submit a pull request.
License

This project is licensed under the MIT License. See the LICENSE file for details.
