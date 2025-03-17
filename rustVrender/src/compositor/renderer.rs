// src/compositor/renderer.rs

use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::{vk, Entry, Instance};
use ash::extensions::{ext::DebugUtils, khr::Surface, khr::Swapchain};
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use winit::window::Window;

pub struct Renderer {
    pub entry: Entry,
    pub instance: Instance,
    pub debug_utils: Option<(DebugUtils, vk::DebugUtilsMessengerEXT)>,
    pub surface_loader: Surface,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub graphics_queue: vk::Queue,
    pub graphics_queue_family_index: u32,
    pub swapchain_loader: Swapchain,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_format: vk::Format,
    pub swapchain_extent: vk::Extent2D,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub render_pass: vk::RenderPass,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub image_available_semaphore: vk::Semaphore,
    pub render_finished_semaphore: vk::Semaphore,
    pub in_flight_fence: vk::Fence,
}

impl Renderer {
    /// Creates a new Renderer from the given window.
    pub fn new(window: &Window) -> Result<Self, Box<dyn std::error::Error>> {
        // 1. Load Vulkan entry
        let entry = Entry::new()?;

        // 2. Create Instance (with debug messenger in debug builds)
        let app_name = CString::new("Rust Compositor")?;
        let engine_name = CString::new("No Engine")?;
        let app_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(vk::make_version(1, 0, 0))
            .engine_name(&engine_name)
            .engine_version(vk::make_version(1, 0, 0))
            .api_version(vk::make_version(1, 1, 0));

        // Determine required extensions from the windowing system
        let mut extension_names = ash_window::enumerate_required_extensions(window)?
            .iter()
            .map(|ext| ext.as_ptr())
            .collect::<Vec<_>>();

        let enable_validation_layers = cfg!(debug_assertions);
        if enable_validation_layers {
            extension_names.push(DebugUtils::name().as_ptr());
        }

        let layer_names: Vec<CString> = if enable_validation_layers {
            vec![CString::new("VK_LAYER_KHRONOS_validation")?]
        } else {
            Vec::new()
        };
        let layer_name_pointers: Vec<*const i8> = layer_names.iter().map(|ln| ln.as_ptr()).collect();

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .enabled_layer_names(&layer_name_pointers);
        let instance = unsafe { entry.create_instance(&create_info, None)? };

        // 3. Setup Debug Messenger if validation layers are enabled
        let debug_utils = if enable_validation_layers {
            let debug_utils_loader = DebugUtils::new(&entry, &instance);
            let messenger_ci = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING |
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL |
                    vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION |
                    vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(vulkan_debug_callback));
            let messenger = unsafe { debug_utils_loader.create_debug_utils_messenger(&messenger_ci, None)? };
            Some((debug_utils_loader, messenger))
        } else {
            None
        };

        // 4. Create Surface from the provided window.
        let surface = unsafe { ash_window::create_surface(&entry, &instance, window, None)? };
        let surface_loader = Surface::new(&entry, &instance);

        // 5. Select a Physical Device that supports graphics and presentation
        let physical_devices = unsafe { instance.enumerate_physical_devices()? };
        let (physical_device, graphics_queue_family_index) = physical_devices
            .iter()
            .filter_map(|&device| {
                let queue_families = unsafe { instance.get_physical_device_queue_family_properties(device) };
                queue_families.iter().enumerate().find_map(|(index, info)| {
                    let supports_graphics = info.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                    let supports_surface = unsafe {
                        surface_loader.get_physical_device_surface_support(device, index as u32, surface).unwrap_or(false)
                    };
                    if supports_graphics && supports_surface {
                        Some((device, index as u32))
                    } else {
                        None
                    }
                })
            })
            .next()
            .ok_or("Failed to find a suitable physical device with required queue support.")?;

        // 6. Create Logical Device and retrieve the graphics queue.
        let queue_priority = 1.0_f32;
        let queue_info = [vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(graphics_queue_family_index)
            .queue_priorities(&[queue_priority])
            .build()];
        let device_extension_names_raw = [Swapchain::name().as_ptr()];
        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&device_extension_names_raw);
        let device = unsafe { instance.create_device(physical_device, &device_create_info, None)? };
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family_index, 0) };

        // 7. Create the Swapchain
        let swapchain_loader = Swapchain::new(&instance, &device);
        let (swapchain, swapchain_images, swapchain_image_format, swapchain_extent) =
            Self::create_swapchain(&instance, &device, physical_device, surface, &surface_loader, window, graphics_queue_family_index)?;
        
        // 8. Create Image Views for each Swapchain image.
        let swapchain_image_views = Self::create_image_views(&device, &swapchain_images, swapchain_image_format)?;

        // 9. Create a Render Pass that clears the screen.
        let render_pass = Self::create_render_pass(&device, swapchain_image_format)?;

        // 10. Create a Command Pool for allocating command buffers.
        let command_pool = Self::create_command_pool(&device, graphics_queue_family_index)?;

        // 11. Allocate Command Buffers (one per swapchain image)
        let command_buffers = Self::create_command_buffers(&device, command_pool, swapchain_image_views.len() as u32)?;

        // 12. Record the Command Buffers (each clears the screen)
        Self::record_command_buffers(&device, render_pass, swapchain_extent, &swapchain_image_views, &command_buffers)?;

        // 13. Create synchronization objects.
        let (image_available_semaphore, render_finished_semaphore, in_flight_fence) = Self::create_sync_objects(&device)?;

        Ok(Renderer {
            entry,
            instance,
            debug_utils,
            surface_loader,
            surface,
            physical_device,
            device,
            graphics_queue,
            graphics_queue_family_index,
            swapchain_loader,
            swapchain,
            swapchain_images,
            swapchain_image_format,
            swapchain_extent,
            swapchain_image_views,
            render_pass,
            command_pool,
            command_buffers,
            image_available_semaphore,
            render_finished_semaphore,
            in_flight_fence,
        })
    }

    fn create_swapchain(
        instance: &Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
        surface_loader: &Surface,
        window: &Window,
        graphics_queue_family_index: u32,
    ) -> Result<(vk::SwapchainKHR, Vec<vk::Image>, vk::Format, vk::Extent2D), Box<dyn std::error::Error>> {
        // Query surface capabilities and available formats.
        let surface_capabilities = unsafe { surface_loader.get_physical_device_surface_capabilities(physical_device, surface)? };
        let surface_formats = unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };
        let surface_format = surface_formats
            .iter()
            .cloned()
            .find(|sf| sf.format == vk::Format::B8G8R8A8_SRGB && sf.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .unwrap_or(surface_formats[0]);
        let swapchain_image_format = surface_format.format;

        // Determine the swapchain extent (resolution).
        let swapchain_extent = if surface_capabilities.current_extent.width != u32::MAX {
            surface_capabilities.current_extent
        } else {
            let window_size = window.inner_size();
            vk::Extent2D {
                width: window_size.width.clamp(surface_capabilities.min_image_extent.width, surface_capabilities.max_image_extent.width),
                height: window_size.height.clamp(surface_capabilities.min_image_extent.height, surface_capabilities.max_image_extent.height),
            }
        };

        // Decide on the number of images for buffering.
        let mut image_count = surface_capabilities.min_image_count + 1;
        if surface_capabilities.max_image_count > 0 && image_count > surface_capabilities.max_image_count {
            image_count = surface_capabilities.max_image_count;
        }

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(swapchain_image_format)
            .image_color_space(surface_format.color_space)
            .image_extent(swapchain_extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO) // FIFO is guaranteed to be available.
            .clipped(true);
        let swapchain = unsafe { Swapchain::new(instance, device).create_swapchain(&swapchain_create_info, None)? };
        let swapchain_images = unsafe { Swapchain::new(instance, device).get_swapchain_images(swapchain)? };

        Ok((swapchain, swapchain_images, swapchain_image_format, swapchain_extent))
    }

    fn create_image_views(
        device: &ash::Device,
        images: &[vk::Image],
        format: vk::Format,
    ) -> Result<Vec<vk::ImageView>, Box<dyn std::error::Error>> {
        let mut image_views = Vec::with_capacity(images.len());
        for &image in images {
            let create_view_info = vk::ImageViewCreateInfo::builder()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::IDENTITY,
                    g: vk::ComponentSwizzle::IDENTITY,
                    b: vk::ComponentSwizzle::IDENTITY,
                    a: vk::ComponentSwizzle::IDENTITY,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let image_view = unsafe { device.create_image_view(&create_view_info, None)? };
            image_views.push(image_view);
        }
        Ok(image_views)
    }

    fn create_render_pass(device: &ash::Device, format: vk::Format) -> Result<vk::RenderPass, Box<dyn std::error::Error>> {
        let color_attachment = vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();

        let color_attachment_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };

        let subpass = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&color_attachment_ref))
            .build();

        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(std::slice::from_ref(&color_attachment))
            .subpasses(std::slice::from_ref(&subpass));

        let render_pass = unsafe { device.create_render_pass(&render_pass_info, None)? };
        Ok(render_pass)
    }

    fn create_command_pool(device: &ash::Device, queue_family_index: u32) -> Result<vk::CommandPool, Box<dyn std::error::Error>> {
        let pool_info = vk::CommandPoolCreateInfo::builder().queue_family_index(queue_family_index);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None)? };
        Ok(command_pool)
    }

    fn create_command_buffers(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        count: u32,
    ) -> Result<Vec<vk::CommandBuffer>, Box<dyn std::error::Error>> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(count);
        let command_buffers = unsafe { device.allocate_command_buffers(&alloc_info)? };
        Ok(command_buffers)
    }

    fn record_command_buffers(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        extent: vk::Extent2D,
        image_views: &[vk::ImageView],
        command_buffers: &[vk::CommandBuffer],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Record a command buffer for each swapchain image.
        for (i, &command_buffer) in command_buffers.iter().enumerate() {
            let begin_info = vk::CommandBufferBeginInfo::builder();
            unsafe {
                device.begin_command_buffer(command_buffer, &begin_info)?;
            }
            
            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.1, 0.2, 0.3, 1.0] },
            }];
            
            // Create a framebuffer on the fly for recording.
            let framebuffer = {
                let framebuffer_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(render_pass)
                    .attachments(std::slice::from_ref(&image_views[i]))
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1);
                unsafe { device.create_framebuffer(&framebuffer_info, None)? }
            };
            
            let render_pass_info = vk::RenderPassBeginInfo::builder()
                .render_pass(render_pass)
                .framebuffer(framebuffer)
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                })
                .clear_values(&clear_values);
            
            unsafe {
                device.cmd_begin_render_pass(command_buffer, &render_pass_info, vk::SubpassContents::INLINE);
                // Additional drawing commands would be recorded here.
                device.cmd_end_render_pass(command_buffer);
                device.end_command_buffer(command_buffer)?;
            }
            // Destroy the temporary framebuffer.
            unsafe { device.destroy_framebuffer(framebuffer, None); }
        }
        Ok(())
    }
    
    fn create_sync_objects(device: &ash::Device) -> Result<(vk::Semaphore, vk::Semaphore, vk::Fence), Box<dyn std::error::Error>> {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        let image_available_semaphore = unsafe { device.create_semaphore(&semaphore_info, None)? };
        let render_finished_semaphore = unsafe { device.create_semaphore(&semaphore_info, None)? };
        let in_flight_fence = unsafe { device.create_fence(&fence_info, None)? };
        Ok((image_available_semaphore, render_finished_semaphore, in_flight_fence))
    }

    /// Draws a single frame:
    /// - Waits for the previous frame to finish.
    /// - Acquires the next swapchain image.
    /// - Submits the corresponding command buffer.
    /// - Presents the image.
    pub fn draw_frame(&self) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.device.wait_for_fences(&[self.in_flight_fence], true, std::u64::MAX)?;
            self.device.reset_fences(&[self.in_flight_fence])?;
        }
        
        let (image_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(self.swapchain, std::u64::MAX, self.image_available_semaphore, vk::Fence::null())?
        };

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(std::slice::from_ref(&self.image_available_semaphore))
            .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .command_buffers(std::slice::from_ref(&self.command_buffers[image_index as usize]))
            .signal_semaphores(std::slice::from_ref(&self.render_finished_semaphore))
            .build();

        unsafe {
            self.device.queue_submit(self.graphics_queue, &[submit_info], self.in_flight_fence)?;
        }

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(std::slice::from_ref(&self.render_finished_semaphore))
            .swapchains(std::slice::from_ref(&self.swapchain))
            .image_indices(std::slice::from_ref(&image_index));
        unsafe {
            self.swapchain_loader.queue_present(self.graphics_queue, &present_info)?;
        }
        Ok(())
    }
}

/// Vulkan debug callback (active when validation layers are enabled)
unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    let message = if !p_callback_data.is_null() {
        CStr::from_ptr((*p_callback_data).p_message).to_string_lossy().into_owned()
    } else {
        String::from("No message")
    };
    println!("[VULKAN DEBUG] [{:?} {:?}] : {}", message_severity, message_types, message);
    vk::FALSE
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().expect("Failed to wait for device idle");
            self.device.destroy_semaphore(self.image_available_semaphore, None);
            self.device.destroy_semaphore(self.render_finished_semaphore, None);
            self.device.destroy_fence(self.in_flight_fence, None);
            for &image_view in self.swapchain_image_views.iter() {
                self.device.destroy_image_view(image_view, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.device.destroy_device(None);
            if let Some((ref debug_utils, messenger)) = self.debug_utils {
                debug_utils.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::event_loop::EventLoop;
    use winit::window::WindowBuilder;

    #[test]
    fn test_renderer_draw_frame() {
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new().build(&event_loop).unwrap();
        let renderer = Renderer::new(&window).expect("Failed to create renderer");
        renderer.draw_frame().expect("Failed to draw frame");
    }
}
