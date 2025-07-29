use std::{ops::Deref, sync::Arc};

use windows::Win32::Graphics::{
    Direct3D12::*,
    Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_UNKNOWN},
};

use crate::gpulib::GPULib;

#[derive(Debug, Clone, Copy)]
pub enum BufferLocation {
    CPU,
    GPU,
}

pub struct VectorConstantBuffer<T> {
    lib: Arc<GPULib>,
    resource: ID3D12Resource,
    max_size: usize,
    current_len: usize,
    location: BufferLocation,
    data_type: std::marker::PhantomData<T>,
}

impl<T> VectorConstantBuffer<T> {
    pub fn new(
        lib: Arc<GPULib>,
        initial_size: usize,
        location: BufferLocation,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let resource = Self::create_resource(&lib, initial_size, location)?;

        Ok(VectorConstantBuffer {
            lib,
            resource,
            max_size: initial_size,
            current_len: 0,
            location,
            data_type: std::marker::PhantomData,
        })
    }

    fn create_resource(
        lib: &GPULib,
        count: usize,
        location: BufferLocation,
    ) -> Result<ID3D12Resource, Box<dyn std::error::Error>> {
        let heap_properties = D3D12_HEAP_PROPERTIES {
            Type: match location {
                BufferLocation::CPU => D3D12_HEAP_TYPE_UPLOAD,
                BufferLocation::GPU => D3D12_HEAP_TYPE_GPU_UPLOAD,
            },
            ..Default::default()
        };

        let resource_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: (count * size_of::<T>()).try_into()?,
            Height: 1,
            DepthOrArraySize: 1,
            Alignment: 0,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAGS::default(),
        };

        let mut resource_option = None;
        unsafe {
            lib.device.CreateCommittedResource(
                &heap_properties,
                D3D12_HEAP_FLAGS::default(),
                &resource_desc,
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut resource_option,
            )
        }?;

        resource_option.ok_or("Failed to create resource for vector constant buffer".into())
    }

    pub fn len(&self) -> usize {
        self.current_len
    }

    pub fn vertex_buffer_view(&self) -> D3D12_VERTEX_BUFFER_VIEW {
        D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: unsafe { self.resource.GetGPUVirtualAddress() },
            SizeInBytes: (self.current_len * size_of::<T>()) as u32,
            StrideInBytes: size_of::<T>() as u32,
        }
    }

    pub fn index_buffer_view(&self, format: DXGI_FORMAT) -> D3D12_INDEX_BUFFER_VIEW {
        D3D12_INDEX_BUFFER_VIEW {
            BufferLocation: unsafe { self.resource.GetGPUVirtualAddress() },
            SizeInBytes: (self.current_len * size_of::<T>()) as u32,
            Format: format,
        }
    }
}

impl<T: Clone> VectorConstantBuffer<T> {
    pub fn upload(&mut self, data: &[T]) -> Result<(), Box<dyn std::error::Error>> {
        if self.max_size < data.len() {
            self.resource = Self::create_resource(&self.lib, data.len(), self.location)?;
        }
        self.current_len = data.len();

        unsafe {
            let mut pointer = std::ptr::null_mut();
            self.resource.Map(0, None, Some(&mut pointer))?;
            let slice = std::slice::from_raw_parts_mut(pointer as *mut T, data.len());
            slice.clone_from_slice(data);
            self.resource.Unmap(0, None);
        }

        Ok(())
    }
}

impl<T> Deref for VectorConstantBuffer<T> {
    type Target = ID3D12Resource;
    fn deref(&self) -> &Self::Target {
        &self.resource
    }
}
