use egui::{TextureId, epaint::ImageDelta};
use std::mem::ManuallyDrop;
use std::ptr::null_mut;
use std::{collections::HashMap, sync::Arc};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN};
use windows::core::Interface;

use crate::gpulib::GPULib;

pub struct TextureManager {
    textures: HashMap<u64, (ID3D12Resource, ID3D12DescriptorHeap)>,
    command_allocator: ID3D12CommandAllocator,
    command_list: ID3D12GraphicsCommandList,
    fence: ID3D12Fence,
    lib: Arc<GPULib>,
}

impl TextureManager {
    pub fn new(lib: Arc<GPULib>) -> Result<Self, Box<dyn std::error::Error>> {
        let textures = HashMap::new();

        let command_allocator = unsafe {
            lib.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        }?;
        let command_list = unsafe {
            lib.device.CreateCommandList1(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                D3D12_COMMAND_LIST_FLAG_NONE,
            )
        }?;

        let fence = unsafe { lib.device.CreateFence(1, D3D12_FENCE_FLAG_NONE) }?;

        Ok(TextureManager {
            textures,
            command_allocator,
            command_list,
            fence,
            lib,
        })
    }

    pub fn get_descriptor_heap(&self, texture: u64) -> Option<&ID3D12DescriptorHeap> {
        self.textures.get(&texture).map(|texture| &texture.1)
    }

    pub fn set(
        &mut self,
        delta: &[(TextureId, ImageDelta)],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (id, delta) in delta.iter().filter_map(|(id, delta)| match id {
            TextureId::Managed(id) => Some((id, delta)),
            TextureId::User(_) => None, // Not supported yet
        }) {
            let width = delta.image.width() as u32;
            let height = delta.image.height() as u32;

            // Row size aligned to 256 bytes
            let aligned_row_bytes = 4 * width + D3D12_TEXTURE_DATA_PITCH_ALIGNMENT
                - (4 * width) % D3D12_TEXTURE_DATA_PITCH_ALIGNMENT;

            let upload_buffer =
                Self::create_upload_buffer(&self.lib, aligned_row_bytes as u64 * height as u64)?;

            Self::fill_buffer_aligned(&upload_buffer, &delta.image, aligned_row_bytes)?;

            let mut new_resource = false;
            let (destination_textue, _) = self
                .textures
                .entry(*id)
                .or_insert_with(|| {
                    new_resource = true;
                    let texture = Self::create_texture(
                        &self.lib,
                        width,
                        delta.image.height() as u32,
                    )
                    .unwrap();
                    let heap = Self::create_heap_for_texture(&self.lib, &texture).unwrap();
                    (texture, heap)
                })
                .clone();

            let source = D3D12_TEXTURE_COPY_LOCATION {
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                pResource: ManuallyDrop::new(Some(upload_buffer)),
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                            Width: width,
                            Height: height,
                            Depth: 1,
                            RowPitch: aligned_row_bytes,
                        },
                    },
                },
            };

            let destination = D3D12_TEXTURE_COPY_LOCATION {
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                pResource: ManuallyDrop::new(Some(destination_textue.clone())),
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };

            let soruce_box = D3D12_BOX {
                left: 0,
                right: width,
                top: 0,
                bottom: height,
                front: 0,
                back: 1,
            };

            let [dst_x, dst_y] = delta.pos.unwrap_or([0, 0]);

            unsafe {
                self.lib.queue.Wait(&self.fence, 1)?;
                self.fence.Signal(0)?;

                self.command_allocator.Reset()?;
                self.command_list.Reset(&self.command_allocator, None)?;

                self.command_list.ResourceBarrier(&[D3D12_RESOURCE_BARRIER {
                    Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                    Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                    Anonymous: D3D12_RESOURCE_BARRIER_0 {
                        Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                            pResource: ManuallyDrop::new(Some(destination_textue.clone())),
                            Subresource: 0,
                            StateBefore: match new_resource {
                                true => D3D12_RESOURCE_STATE_COMMON,
                                false => D3D12_RESOURCE_STATE_GENERIC_READ,
                            },
                            StateAfter: D3D12_RESOURCE_STATE_COPY_DEST,
                        }),
                    },
                }]);
                self.command_list.CopyTextureRegion(
                    &destination,
                    dst_x as u32,
                    dst_y as u32,
                    0,
                    &source,
                    Some(&soruce_box),
                );
                self.command_list.ResourceBarrier(&[D3D12_RESOURCE_BARRIER {
                    Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                    Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                    Anonymous: D3D12_RESOURCE_BARRIER_0 {
                        Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                            pResource: ManuallyDrop::new(Some(destination_textue.clone())),
                            Subresource: 0,
                            StateBefore: D3D12_RESOURCE_STATE_COPY_DEST,
                            StateAfter: D3D12_RESOURCE_STATE_GENERIC_READ,
                        }),
                    },
                }]);

                self.command_list.Close()?;
                self.lib
                    .queue
                    .ExecuteCommandLists(&[Some(self.command_list.cast()?)]);
                self.lib.queue.Signal(&self.fence, 1)?;
            }
        }
        unsafe { self.lib.queue.Wait(&self.fence, 1) }?;

        Ok(())
    }

    pub fn free(&mut self, textures: &[TextureId]) {
        for id in textures.iter().filter_map(|id| match id {
            TextureId::Managed(id) => Some(id),
            TextureId::User(_) => None, // Not supported yet
        }) {
            self.textures.remove(id);
        }
    }

    fn create_texture(
        lib: &GPULib,
        width: u32,
        height: u32,
    ) -> Result<ID3D12Resource, Box<dyn std::error::Error>> {
        let heap_properties = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_GPU_UPLOAD,
            ..Default::default()
        };

        let resource_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: width as u64,
            Height: height,
            DepthOrArraySize: 1,
            Alignment: 0,
            MipLevels: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
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

        resource_option.ok_or("Failed to create texture resource".into())
    }

    fn create_upload_buffer(
        lib: &GPULib,
        bytes: u64,
    ) -> Result<ID3D12Resource, Box<dyn std::error::Error>> {
        let heap_properties = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_UPLOAD,
            ..Default::default()
        };

        let resource_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
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

        resource_option.ok_or("Failed to create texture upload buffer".into())
    }

    /// Fills an upload buffer with texture data and aligns it's rows to the specified byte count
    fn fill_buffer_aligned(
        buffer: &ID3D12Resource,
        image_data: &egui::ImageData,
        aligned_row_bytes: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let width = image_data.width();
        let height = image_data.height();
        unsafe {
            // Get mapped slice to upload texture memory
            let mut ptr = null_mut();
            buffer.Map(0, None, Some(&mut ptr))?;
            let mapped_slice = std::slice::from_raw_parts_mut(
                ptr as *mut u8,
                aligned_row_bytes as usize * height * 4,
            );

            // Fill texture with image data
            match &image_data {
                egui::ImageData::Color(color_image) => {
                    let src_slice = color_image.as_raw();
                    for row in 0..height {
                        let dst_row_start = aligned_row_bytes as usize * row;
                        let dst_row_end = dst_row_start + width * 4;
                        let src_row_start = width * 4 * row;
                        let src_row_end = src_row_start + width * 4;
                        mapped_slice[dst_row_start..dst_row_end]
                            .copy_from_slice(&src_slice[src_row_start..src_row_end]);
                    }
                }
                egui::ImageData::Font(font_image) => {
                    for (i, pixel) in font_image
                        .srgba_pixels(None)
                        .map(|pixel| [pixel.r(), pixel.g(), pixel.b(), pixel.a()])
                        .enumerate()
                    {
                        let x = i % width;
                        let y = i / width;
                        let dst_index = y * aligned_row_bytes as usize + x * 4;
                        mapped_slice[dst_index..(dst_index + 4)].copy_from_slice(&pixel);
                    }
                }
            };

            buffer.Unmap(0, None);
        }

        Ok(())
    }

    fn create_heap_for_texture(
        lib: &GPULib,
        texture: &ID3D12Resource,
    ) -> Result<ID3D12DescriptorHeap, Box<dyn std::error::Error>> {
        let heap: ID3D12DescriptorHeap = unsafe {
            lib.device
                .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    NumDescriptors: 1,
                    NodeMask: 0,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                })
        }?;
        unsafe {
            lib.device.CreateShaderResourceView(
                texture,
                None,
                heap.GetCPUDescriptorHandleForHeapStart(),
            )
        };

        Ok(heap)
    }
}
