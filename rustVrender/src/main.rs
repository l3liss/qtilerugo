use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use serde::Deserialize;
use serde_json;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

// Vulkan and Vulkano imports:
use vulkano::VulkanLibrary;
use vulkano::instance::{Instance, InstanceCreateInfo, InstanceExtensions};
use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, QueueCreateInfo};
use vulkano::swapchain::Surface; // Use the new API

/// Defines commands that the window renderer understands.
#[derive(Debug, Deserialize)]
enum RendererCommand {
    SpawnWindow,
    // You can add more commands here.
}

/// Listens for JSON-encoded commands on a Unix socket.
async fn listen_for_commands(socket_path: &str) -> tokio::io::Result<()> {
    if Path::new(socket_path).exists() {
        fs::remove_file(socket_path).expect("failed to remove existing socket file");
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
                match serde_json::from_str::<RendererCommand>(trimmed) {
                    Ok(RendererCommand::SpawnWindow) => {
                        println!("Spawning window...");
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

    // Create Vulkan instance.
    let library = VulkanLibrary::new().expect("failed to load Vulkan library");
    let instance = Instance::new(
        library,
        InstanceCreateInfo {
            enabled_extensions: InstanceExtensions::empty(),
            ..Default::default()
        },
    )
    .expect("failed to create Vulkan instance");

    // Create a Vulkan surface from the window using the new API.
    let surface = Surface::from_window(instance.clone(), window.clone())
        .expect("failed to create Vulkan surface");

    // Enumerate physical devices.
    let physical = instance
        .enumerate_physical_devices()
        .expect("Failed to enumerate physical devices")
        .next()
        .expect("No physical device found");

    // Find a queue family that supports graphics and presentation.
    let queue_family = physical.queue_family_properties()
        .iter()
        .enumerate()
        .find(|(index, q)| {
            q.queue_flags.contains(vulkano::device::QueueFlags::GRAPHICS)
                && physical.surface_support(*index as u32, &*surface).unwrap_or(false)
        })
        .map(|(index, _)| index as u32)
        .expect("Couldn't find a graphical queue family that supports presentation");

    // Create the queue.
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
