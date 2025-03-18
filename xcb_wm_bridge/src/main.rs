mod config;

use config::Config;
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConfigureWindowAux, StackMode, ConnectionExt},
    rust_connection::RustConnection,
};
use tokio::net::UnixListener;
use tokio::io::AsyncReadExt;
use serde::Deserialize;
use std::path::Path;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
enum WmCommand {
    FocusLeft,
    FocusRight,
    FocusDown,
    FocusUp,
    FocusNext,
    ShuffleLeft,
    ShuffleRight,
    ShuffleDown,
    ShuffleUp,
    GrowLeft,
    GrowRight,
    GrowDown,
    GrowUp,
    Normalize,
    ToggleSplit,
    SpawnTerminal,
    NextLayout,
    KillWindow,
    ToggleFullscreen,
    ToggleFloating,
    ReloadConfig,
    Shutdown,
    SpawnRofi,
}

struct WindowManager {
    conn: RustConnection,
    config: Config,
}

impl WindowManager {
    fn new(conn: RustConnection, config: Config) -> Self {
        Self { conn, config }
    }

    fn handle_command(&self, cmd: WmCommand) -> Result<(), Box<dyn std::error::Error>> {
        let command_str = match cmd {
            WmCommand::FocusLeft => &self.config.commands["focus_left"],
            WmCommand::FocusRight => &self.config.commands["focus_right"],
            WmCommand::FocusDown => &self.config.commands["focus_down"],
            WmCommand::FocusUp => &self.config.commands["focus_up"],
            WmCommand::FocusNext => &self.config.commands["focus_next"],
            WmCommand::ShuffleLeft => &self.config.commands["shuffle_left"],
            WmCommand::ShuffleRight => &self.config.commands["shuffle_right"],
            WmCommand::ShuffleDown => &self.config.commands["shuffle_down"],
            WmCommand::ShuffleUp => &self.config.commands["shuffle_up"],
            WmCommand::GrowLeft => &self.config.commands["grow_left"],
            WmCommand::GrowRight => &self.config.commands["grow_right"],
            WmCommand::GrowDown => &self.config.commands["grow_down"],
            WmCommand::GrowUp => &self.config.commands["grow_up"],
            WmCommand::Normalize => &self.config.commands["normalize"],
            WmCommand::ToggleSplit => &self.config.commands["toggle_split"],
            WmCommand::SpawnTerminal => &self.config.commands["spawn_terminal"],
            WmCommand::NextLayout => &self.config.commands["next_layout"],
            WmCommand::KillWindow => &self.config.commands["kill_window"],
            WmCommand::ToggleFullscreen => &self.config.commands["toggle_fullscreen"],
            WmCommand::ToggleFloating => &self.config.commands["toggle_floating"],
            WmCommand::ReloadConfig => &self.config.commands["reload_config"],
            WmCommand::Shutdown => &self.config.commands["shutdown"],
            WmCommand::SpawnRofi => &self.config.commands["spawn_rofi"],
        };

        println!("Executing command: {}", command_str);
        self.execute_command(command_str)
    }

    fn execute_command(&self, command: &str) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            "move_focus -x -1" => {
                println!("Focusing left");
                // Add logic to focus left
            },
            "move_focus -x 1" => {
                println!("Focusing right");
                // Add logic to focus right
            },
            "spawn alacritty" => {
                println!("Spawning terminal");
                std::process::Command::new("alacritty").spawn()?;
            },
            "close_window" => {
                println!("Killing window");
                if let Some(window) = self.conn.get_input_focus()?.reply()?.focus {
                    self.conn.destroy_window(window)?;
                }
            },
            _ => println!("Unknown command: {}", command),
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = Config::load("wm_config.toml")?;

    // Create X11 connection
    let (conn, _) = RustConnection::connect(None)?;
    let wm = WindowManager::new(conn, config);

    let sock_path = "/tmp/x11rb_wm.sock";

    // Remove existing socket if it exists
    if Path::new(sock_path).exists() {
        std::fs::remove_file(sock_path)?;
        println!("Removed existing socket");
    }

    // Bind to socket
    let listener = UnixListener::bind(sock_path)?;
    println!("Listening on socket: {}", sock_path);

    // Main loop
    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                println!("New client connected");
                let mut buf = [0u8; 1024];
                let mut buffer = Vec::new();  // Buffer for partial data

                loop {
                    match stream.read(&mut buf).await {
                        Ok(n) if n == 0 => {
                            println!("Client disconnected");
                            break;
                        }
                        Ok(n) => {
                            buffer.extend_from_slice(&buf[..n]);  // Append new data to buffer
                            println!("Received raw data: {:?}", &buffer);

                            // Attempt to parse JSON
                            match serde_json::from_slice(&buffer) {
                                Ok(cmd) => {
                                    println!("Parsed command: {:?}", cmd);
                                    if let Err(e) = wm.handle_command(cmd) {
                                        eprintln!("Error handling command: {}", e);
                                    }
                                    buffer.clear();  // Clear buffer after successful parse
                                }
                                Err(e) if e.is_eof() => {
                                    // Incomplete data, wait for more
                                    continue;
                                }
                                Err(e) => {
                                    eprintln!("Invalid command: {} (data: {:?})", e, buffer);
                                    buffer.clear();  // Clear buffer on error
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Read error: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => eprintln!("Connection error: {}", e),
        }
    }
}
