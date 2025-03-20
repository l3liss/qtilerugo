use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use serde::Deserialize;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

// Vulkan and vulkano-win imports:
use vulkano::VulkanLibrary;
use vulkano::instance::{Instance, InstanceCreateInfo, InstanceExtensions};
use vulkano_win::create_surface_from_winit;
use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, QueueCreateInfo};

/// Defines commands that the window renderer understands.
/// Currently, we support only a spawn window command.
#[derive(Debug, Deserialize)]
enum RendererCommand {
    SpawnWindow,
    // You can add more variants here as needed.
}

/// Listens for JSON-encoded commands on a Unix socket.
async fn listen_for_commands(socket_path: &str) -> tokio::io::Result<()> {
    // Remove an existing socket file if it exists.
    if Path::new(socket_path).exists() {
        fs::remove_file(socket_path)
            .expect("failed to remove existing socket file");
    }
    let listener = UnixListener::bind(socket_path)?;
    println!("Listening on Unix socket: {}", socket_path);
    
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let reader = BufReader::new(stream);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                println!("Received raw command: {}", trimmed);
                // Parse the incoming command as JSON.
                match serde_json::from_str::<RendererCommand>(trimmed) {
                    Ok(RendererCommand::SpawnWindow) => {
                        println!("Spawning window...");
                        // Use a new thread because the winit event loop blocks.
                        thread::spawn(|| {
                            create_window();
                        });
                    }
                    Err(e) => {
                        println!("Invalid command: {}. Received: {}", e, trimmed);
                    }
                }
            }
        });
    }
}

/// Creates a window with Vulkan support.
fn create_window() {
    // Create the event loop and window.
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Rust-Created Window")
        .build(&event_loop)
        .expect("Failed to create window");
    let window = Arc::new(window);

    // Create a Vulkan instance.
    let library = VulkanLibrary::new().expect("failed to load Vulkan library");
    let instance = Instance::new(
        library,
        InstanceCreateInfo {
            enabled_extensions: InstanceExtensions::empty(),
            ..Default::default()
        },
    )
    .expect("failed to create Vulkan instance");

    // Create a Vulkan surface from the window.
    let surface = create_surface_from_winit(window.clone(), instance.clone())
        .expect("failed to create Vulkan surface");

    // Find a suitable physical device.
    let physical = instance
        .enumerate_physical_devices()
        .expect("Failed to enumerate physical devices")
        .next()
        .expect("No physical device found");

    // Choose a queue family that supports graphics and presentation.
    let queue_family = physical.queue_family_properties()
        .iter()
        .enumerate()
        .find(|(index, q)| {
            q.queue_flags.contains(vulkano::device::QueueFlags::GRAPHICS)
                && physical.surface_support(*index as u32, &surface).unwrap_or(false)
        })
        .map(|(index, _)| index as u32)
        .expect("Couldn't find a graphical queue family that supports presentation");

    // Create queue info.
    let queue_create_info = QueueCreateInfo {
        queue_family_index: queue_family,
        queues: vec![1.0],
        ..Default::default()
    };

    // Create the logical device.
    let (device, mut queues) = Device::new(
        physical,
        DeviceCreateInfo {
            queue_create_infos: vec![queue_create_info],
            enabled_extensions: DeviceExtensions {
                khr_swapchain: true,
                ..DeviceExtensions::empty()
            },
            ..Default::default()
        },
    )
    .expect("failed to create device");
    let _queue = queues.next().unwrap();

    println!("Created a new window with Vulkan support.");

    // Run the event loop.
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            println!("Window closed.");
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[tokio::main]
async fn main() {
    let socket_path = "/tmp/rust_qtile_helper.sock";
    if let Err(e) = listen_for_commands(socket_path).await {
        eprintln!("Error: {}", e);
    }
}
