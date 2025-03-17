use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

// Vulkan and Vulkano-Win imports:
use vulkano::VulkanLibrary;
use vulkano::instance::{Instance, InstanceCreateInfo, InstanceExtensions};
use vulkano_win::create_surface_from_winit;
use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, QueueFlags, QueueCreateInfo};

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
                let command = line.trim();
                println!("Received command: {}", command);
                if command == "spawn_window" {
                    thread::spawn(|| {
                        create_window();
                    });
                } else {
                    println!("Unknown command: {}", command);
                }
            }
        });
    }
}

fn create_window() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Rust-Created Window")
        .build(&event_loop)
        .expect("Failed to create window");
    let window = Arc::new(window);

    // Create Vulkan instance.
    let library = VulkanLibrary::new().expect("failed to load Vulkan library");
    let instance = Instance::new(library, InstanceCreateInfo {
        enabled_extensions: InstanceExtensions::empty(),
        ..Default::default()
    })
    .expect("failed to create Vulkan instance");

    // Create a Vulkan surface from the window.
    let surface = create_surface_from_winit(window.clone(), instance.clone())
        .expect("failed to create Vulkan surface");

    // Enumerate physical devices using the new API.
    let physical = instance
        .enumerate_physical_devices()
        .expect("Failed to enumerate physical devices")
        .next()
        .expect("No physical device found");

    let queue_family = physical.queue_family_properties()
        .iter()
        .enumerate()
        .find(|(index, q)| {
            q.queue_flags.contains(vulkano::device::QueueFlags::GRAPHICS)
            && physical.surface_support(*index as u32, &surface).unwrap_or(false)
        })
        .map(|(index, _)| index as u32)
        .expect("Couldn't find a graphical queue family that supports presentation");

    // Manually create queue info.
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
        }
    )
    .expect("failed to create device");
    let _queue = queues.next().unwrap();

    println!("Created a new window with Vulkan support.");

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
