use std::slice::from_raw_parts_mut;

use ash::vk;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, Allocator},
    MemoryLocation,
};

pub struct Buffer {
    pub buffer: vk::Buffer,
    pub allocation: Option<Allocation>,
    pub size: u64,
    pub device_addr: u64,
    pub has_been_written_to: bool,
}

impl Buffer {
    pub fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        buffer_info: &vk::BufferCreateInfo,
        location: MemoryLocation,
    ) -> Buffer {
        let size = buffer_info.size;
        let buffer_info = &mut buffer_info.clone();

        if !buffer_info
            .usage
            .contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS)
        {
            buffer_info.usage |= vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        }

        let buffer = unsafe { device.create_buffer(buffer_info, None) }.unwrap();
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name: "buffer",
                requirements,
                location,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .unwrap();

        let device_addr: u64;
        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .unwrap();

            device_addr = device.get_buffer_device_address(&vk::BufferDeviceAddressInfo {
                buffer,
                s_type: vk::StructureType::BUFFER_DEVICE_ADDRESS_INFO,
                p_next: std::ptr::null(),
                ..Default::default()
            });
        };

        Self {
            buffer,
            allocation: Some(allocation),
            size,
            device_addr,
            has_been_written_to: false,
        }
    }

    pub fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        allocator.free(self.allocation.take().unwrap()).unwrap();
        unsafe { device.destroy_buffer(self.buffer, None) };
    }

    pub fn copy_from_slice<T>(&mut self, slice: &[T], offset: usize)
    where
        T: Copy,
    {
        let Some(allocation) = self.allocation.as_ref() else {
            panic!("Tried writing to buffer but buffer not allocated");
        };
        //assert!(std::mem::size_of_val(slice) + offset <= self.info.get_size());

        unsafe {
            let ptr = allocation.mapped_ptr().unwrap().as_ptr() as *mut u8;
            let mem_ptr = ptr.add(offset);
            let mapped_slice = from_raw_parts_mut(mem_ptr as *mut T, slice.len());
            mapped_slice.copy_from_slice(slice);
        }
        self.has_been_written_to = true;
    }
}

pub struct Image {
    pub image: vk::Image,
    pub allocation: Option<Allocation>,
    pub view: Option<vk::ImageView>,
    pub format: vk::Format,
}

impl Image {
    pub fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        image_info: &vk::ImageCreateInfo,
        location: MemoryLocation,
    ) -> Image {
        let image = unsafe { device.create_image(image_info, None) }.unwrap();
        let requirements = unsafe { device.get_image_memory_requirements(image) };

        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name: "image",
                requirements,
                location,
                linear: false,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .unwrap();

        unsafe {
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .unwrap()
        };

        Self {
            image,
            allocation: Some(allocation),
            view: None,
            format: image_info.format,
        }
    }

    pub fn create_view(&mut self, device: &ash::Device) -> vk::ImageView {
        let view = unsafe {
            device.create_image_view(
                &vk::ImageViewCreateInfo {
                    view_type: vk::ImageViewType::TYPE_2D,
                    format: self.format,
                    components: vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    },
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        level_count: 1,
                        layer_count: 1,
                        ..Default::default()
                    },
                    image: self.image,
                    ..Default::default()
                },
                None,
            )
        }
        .unwrap();
        self.view = Some(view);
        view
    }

    pub fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        if let Some(view) = self.view.take() {
            unsafe { device.destroy_image_view(view, None) };
        }
        allocator.free(self.allocation.take().unwrap()).unwrap();
        unsafe { device.destroy_image(self.image, None) };
    }
}
