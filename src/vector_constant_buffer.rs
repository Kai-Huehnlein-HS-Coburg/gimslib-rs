use std::{ops::Deref, sync::Arc};

use windows::{
    Win32::Graphics::{
        Direct3D12::*,
        Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_UNKNOWN},
    },
    core::HSTRING,
};

use crate::gpulib::GPULib;

#[derive(Debug, Clone, Copy)]
pub enum BufferLocation {
    /// CPU memory, mapping is possible
    Cpu,
    /// GPU memory, mapping is possible. Only usable on modern graphics cards with ResizableBAR.
    GpuUpload,
}

/// Direct3D 12 buffer which is automatically resized to fit the data written to it.
/// It will never shrink automatically, so smaller future writes happen immediately
/// without a new allocation. It dereferences to it's internal `ID3D12Resource`.
pub struct VectorConstantBuffer<T> {
    lib: Arc<GPULib>,
    resource: ID3D12Resource,
    max_size: usize,
    current_len: usize,
    location: BufferLocation,
    name: Option<String>,
    data_type: std::marker::PhantomData<T>,
}

impl<T> VectorConstantBuffer<T> {
    /// Constructs a new `VectorConstantBuffer<T>`.
    /// `initial_size` specifies the number of items for which space is reserved. This does not affect the reported length.
    /// `location` specifies whether the buffer is located in GPU or CPU memory.
    pub fn new(
        lib: Arc<GPULib>,
        initial_size: usize,
        location: BufferLocation,
        name: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let resource = Self::create_resource(&lib, initial_size, location, &name)?;

        Ok(VectorConstantBuffer {
            lib,
            resource,
            max_size: initial_size,
            current_len: 0,
            location,
            name,
            data_type: std::marker::PhantomData,
        })
    }

    fn create_resource(
        lib: &GPULib,
        count: usize,
        location: BufferLocation,
        name: &Option<String>,
    ) -> Result<ID3D12Resource, Box<dyn std::error::Error>> {
        let heap_properties = D3D12_HEAP_PROPERTIES {
            Type: match location {
                BufferLocation::Cpu => D3D12_HEAP_TYPE_UPLOAD,
                BufferLocation::GpuUpload => D3D12_HEAP_TYPE_GPU_UPLOAD,
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

        let resource: ID3D12Resource =
            resource_option.ok_or("Failed to create resource for vector constant buffer")?;

        if let Some(name) = name {
            unsafe { resource.SetName(&HSTRING::from(name)) }?;
        }

        Ok(resource)
    }

    /// The number of items currently stored in the buffer
    pub fn len(&self) -> usize {
        self.current_len
    }

    pub fn is_empty(&self) -> bool {
        self.current_len == 0
    }

    /// Creates a `D3D12_VERTEX_BUFFER_VIEW` for the internal `ID3D12Resource`, spanning the buffer's entire current length.
    /// Stride is set to the size of the buffer's type.
    pub fn vertex_buffer_view(&self) -> D3D12_VERTEX_BUFFER_VIEW {
        D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: unsafe { self.resource.GetGPUVirtualAddress() },
            SizeInBytes: (self.current_len * size_of::<T>()) as u32,
            StrideInBytes: size_of::<T>() as u32,
        }
    }

    /// Creates a `D3D12_INDEX_BUFFER_VIEW` for the internal `ID3D12Resource` with the specified format,
    /// spanning the buffer's entire current length.
    pub fn index_buffer_view(&self, format: DXGI_FORMAT) -> D3D12_INDEX_BUFFER_VIEW {
        D3D12_INDEX_BUFFER_VIEW {
            BufferLocation: unsafe { self.resource.GetGPUVirtualAddress() },
            SizeInBytes: (self.current_len * size_of::<T>()) as u32,
            Format: format,
        }
    }
}

impl<T: Clone> VectorConstantBuffer<T> {
    /// Copy new data into the buffer
    pub fn upload(&mut self, data: &[T]) -> Result<(), Box<dyn std::error::Error>> {
        self.upload_deferred_delete(data)?;
        Ok(())
    }

    /// Copy new data into the buffer while returning the potentially discarded previous buffer.
    /// Useful for delete queues tied to frames in flight.
    pub fn upload_deferred_delete(
        &mut self,
        data: &[T],
    ) -> Result<Option<ID3D12Resource>, Box<dyn std::error::Error>> {
        let deleted_resource = if self.max_size < data.len() {
            let new_resource =
                Self::create_resource(&self.lib, data.len(), self.location, &self.name)?;
            Some(std::mem::replace(&mut self.resource, new_resource))
        } else {
            None
        };
        self.current_len = data.len();

        unsafe {
            let mut pointer = std::ptr::null_mut();
            self.resource.Map(0, None, Some(&mut pointer))?;
            let slice = std::slice::from_raw_parts_mut(pointer as *mut T, data.len());
            slice.clone_from_slice(data);
            self.resource.Unmap(0, None);
        }

        Ok(deleted_resource)
    }
}

impl<T> Deref for VectorConstantBuffer<T> {
    type Target = ID3D12Resource;
    fn deref(&self) -> &Self::Target {
        &self.resource
    }
}
