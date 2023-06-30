use std::{
    collections::{BTreeMap, HashMap},
    ffi::{CStr, CString},
};

use ash::vk::{self, SamplerCreateInfo, ShaderCodeTypeEXT, ShaderCreateInfoEXT};

use crate::{chunky_list::TempList, ctx::SamplerDesc};

use super::RenderInstance;

pub struct Shader {
    pub kind: ShaderKind,
    pub spirv: Vec<u8>,
    pub spirv_descripor_set_layouts: StageDescriptorSetLayouts,
    entry_point: String,
    entry_point_cstr: CString,
}

pub enum ShaderKind {
    Vertex,
    Fragment,
    Compute,
}
impl ShaderKind {
    pub fn to_shaderc_kind(&self) -> shaderc::ShaderKind {
        match self {
            Self::Vertex => shaderc::ShaderKind::Vertex,
            Self::Fragment => shaderc::ShaderKind::Fragment,
            Self::Compute => shaderc::ShaderKind::Compute,
        }
    }

    pub fn to_vk_shader_stage_flag(&self) -> vk::ShaderStageFlags {
        match self {
            Self::Vertex => vk::ShaderStageFlags::VERTEX,
            Self::Fragment => vk::ShaderStageFlags::FRAGMENT,
            Self::Compute => vk::ShaderStageFlags::COMPUTE,
        }
    }
}

type DescriptorSetLayout = BTreeMap<u32, rspirv_reflect::DescriptorInfo>;
type StageDescriptorSetLayouts = BTreeMap<u32, DescriptorSetLayout>;

impl Shader {
    pub fn new(spirv: &[u8], kind: ShaderKind, entry_point: &str) -> Self {
        let refl_info = rspirv_reflect::Reflection::new_from_spirv(spirv).unwrap();
        let descriptor_sets = refl_info.get_descriptor_sets().unwrap();

        Self {
            kind,
            spirv_descripor_set_layouts: descriptor_sets,
            entry_point: entry_point.to_string(),
            spirv: spirv.to_vec(),
            entry_point_cstr: CString::new(entry_point).unwrap(),
        }
    }

    pub fn create_descriptor_sets(
        &self,
        render_instance: &RenderInstance,
        descriptor_set_layouts: &Vec<vk::DescriptorSetLayout>,
        set_layout_info: &Vec<HashMap<u32, vk::DescriptorType>>,
    ) -> Vec<vk::DescriptorSet> {
        let mut descriptor_pool_sizes: Vec<vk::DescriptorPoolSize> = Vec::new();
        for bindings in set_layout_info.iter() {
            for ty in bindings.values() {
                if let Some(mut dps) = descriptor_pool_sizes.iter_mut().find(|item| item.ty == *ty)
                {
                    dps.descriptor_count += 1;
                } else {
                    descriptor_pool_sizes.push(vk::DescriptorPoolSize {
                        ty: *ty,
                        descriptor_count: 1,
                    })
                }
            }
        }

        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&descriptor_pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe {
            render_instance
                .device()
                .create_descriptor_pool(&descriptor_pool_info, None)
                .unwrap()
        };

        let desc_alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&descriptor_set_layouts);
        let descriptor_sets = unsafe {
            render_instance
                .device()
                .allocate_descriptor_sets(&desc_alloc_info)
                .unwrap()
        };

        descriptor_sets
    }

    pub fn ext_shader_create_info(&self) -> ShaderCreateInfoEXT {
        ShaderCreateInfoEXT::default()
            .name(self.entry_point_cstr.as_c_str())
            .code(&self.spirv)
            .code_type(ShaderCodeTypeEXT::SPIRV)
            .stage(self.kind.to_vk_shader_stage_flag())
    }

    pub fn create_descriptor_set_layouts(
        &self,
        render_instance: &RenderInstance,
    ) -> (
        Vec<vk::DescriptorSetLayout>,
        Vec<HashMap<u32, vk::DescriptorType>>,
    ) {
        let samplers = TempList::new();
        let set_count = self
            .spirv_descripor_set_layouts
            .keys()
            .map(|set_index| *set_index + 1)
            .max()
            .unwrap_or(0u32);

        let mut set_layouts: Vec<vk::DescriptorSetLayout> = Vec::with_capacity(set_count as usize);
        let mut set_layout_info: Vec<HashMap<u32, vk::DescriptorType>> =
            Vec::with_capacity(set_count as usize);

        for set_index in 0..set_count {
            let stage_flags = vk::ShaderStageFlags::ALL;
            let set = self.spirv_descripor_set_layouts.get(&set_index);

            if let Some(set) = set {
                let mut bindings: Vec<vk::DescriptorSetLayoutBinding> =
                    Vec::with_capacity(set.len());
                let mut binding_flags: Vec<vk::DescriptorBindingFlags> =
                    vec![vk::DescriptorBindingFlags::PARTIALLY_BOUND; set.len()];

                let mut set_layout_create_flags = vk::DescriptorSetLayoutCreateFlags::empty();

                for (binding_index, binding) in set.iter() {
                    match binding.ty {
                        rspirv_reflect::DescriptorType::UNIFORM_BUFFER
                        | rspirv_reflect::DescriptorType::UNIFORM_TEXEL_BUFFER
                        | rspirv_reflect::DescriptorType::STORAGE_IMAGE
                        | rspirv_reflect::DescriptorType::STORAGE_BUFFER
                        | rspirv_reflect::DescriptorType::STORAGE_BUFFER_DYNAMIC => bindings.push(
                            vk::DescriptorSetLayoutBinding::default()
                                .binding(*binding_index)
                                //.descriptor_count(binding.count)
                                .descriptor_count(1) // TODO
                                .descriptor_type(match binding.ty {
                                    rspirv_reflect::DescriptorType::UNIFORM_BUFFER => {
                                        vk::DescriptorType::UNIFORM_BUFFER
                                    }
                                    rspirv_reflect::DescriptorType::UNIFORM_BUFFER_DYNAMIC => {
                                        vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                                    }
                                    rspirv_reflect::DescriptorType::UNIFORM_TEXEL_BUFFER => {
                                        vk::DescriptorType::UNIFORM_TEXEL_BUFFER
                                    }
                                    rspirv_reflect::DescriptorType::STORAGE_IMAGE => {
                                        vk::DescriptorType::STORAGE_IMAGE
                                    }
                                    rspirv_reflect::DescriptorType::STORAGE_BUFFER => {
                                        if binding.name.ends_with("_dyn") {
                                            vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                        } else {
                                            vk::DescriptorType::STORAGE_BUFFER
                                        }
                                    }
                                    rspirv_reflect::DescriptorType::STORAGE_BUFFER_DYNAMIC => {
                                        vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                                    }
                                    _ => unimplemented!("{:?}", binding),
                                })
                                .stage_flags(stage_flags),
                        ),
                        rspirv_reflect::DescriptorType::SAMPLED_IMAGE => {
                            // if matches!(
                            //     binding.dimensionality,
                            //     rspirv_reflect::DescriptorDimensionality::RuntimeArray
                            // ) {
                            //     // Bindless

                            //     binding_flags[bindings.len()] =
                            //         vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
                            //             | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING
                            //             | vk::DescriptorBindingFlags::PARTIALLY_BOUND
                            //             | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT;

                            //     set_layout_create_flags |=
                            //         vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL;
                            // }

                            // let descriptor_count = match binding.ty {
                            //     rspirv_reflect::DescriptorType::Single => 1,
                            //     rspirv_reflect::DescriptorDimensionality::Array(size) => size,
                            //     rspirv_reflect::DescriptorDimensionality::RuntimeArray => {
                            //         device.max_bindless_descriptor_count()
                            //     }
                            // };

                            bindings.push(
                                vk::DescriptorSetLayoutBinding::default()
                                    .binding(*binding_index)
                                    .descriptor_count(1) // TODO
                                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                                    .stage_flags(stage_flags),
                            );
                        }
                        rspirv_reflect::DescriptorType::SAMPLER => {
                            let name_prefix = "sampler_";
                            if let Some(mut spec) = binding.name.strip_prefix(name_prefix) {
                                let texel_filter = match &spec[..1] {
                                    "n" => vk::Filter::NEAREST,
                                    "l" => vk::Filter::LINEAR,
                                    _ => panic!("{}", &spec[..1]),
                                };
                                spec = &spec[1..];

                                let mipmap_mode = match &spec[..1] {
                                    "n" => vk::SamplerMipmapMode::NEAREST,
                                    "l" => vk::SamplerMipmapMode::LINEAR,
                                    _ => panic!("{}", &spec[..1]),
                                };
                                spec = &spec[1..];

                                let address_modes = match spec {
                                    "r" => vk::SamplerAddressMode::REPEAT,
                                    "mr" => vk::SamplerAddressMode::MIRRORED_REPEAT,
                                    "c" => vk::SamplerAddressMode::CLAMP_TO_EDGE,
                                    "cb" => vk::SamplerAddressMode::CLAMP_TO_BORDER,
                                    _ => panic!("{}", spec),
                                };

                                let renderer = &render_instance.0;
                                bindings.push(
                                    vk::DescriptorSetLayoutBinding::default()
                                        .descriptor_count(1)
                                        .descriptor_type(vk::DescriptorType::SAMPLER)
                                        .stage_flags(stage_flags)
                                        .binding(*binding_index)
                                        .immutable_samplers(std::slice::from_ref(samplers.add(
                                            renderer.get_sampler(SamplerDesc {
                                                texel_filter,
                                                mipmap_mode,
                                                address_modes,
                                            }),
                                        ))),
                                );
                            } else {
                                panic!("{}", binding.name);
                            }
                        }
                        rspirv_reflect::DescriptorType::ACCELERATION_STRUCTURE_KHR => bindings
                            .push(
                                vk::DescriptorSetLayoutBinding::default()
                                    .binding(*binding_index)
                                    .descriptor_count(1) // TODO
                                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                                    .stage_flags(stage_flags),
                            ),

                        _ => unimplemented!("{:?}", binding),
                    }
                }

                let mut binding_flags_create_info =
                    vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                        .binding_flags(&binding_flags);

                let set_layout = unsafe {
                    render_instance
                        .device()
                        .create_descriptor_set_layout(
                            &vk::DescriptorSetLayoutCreateInfo::default()
                                .flags(set_layout_create_flags)
                                .bindings(&bindings)
                                .push_next(&mut binding_flags_create_info),
                            None,
                        )
                        .unwrap()
                };

                set_layouts.push(set_layout);
                set_layout_info.push(
                    bindings
                        .iter()
                        .map(|binding| (binding.binding, binding.descriptor_type))
                        .collect(),
                );
            } else {
                let set_layout = unsafe {
                    render_instance
                        .device()
                        .create_descriptor_set_layout(
                            &vk::DescriptorSetLayoutCreateInfo::default(),
                            None,
                        )
                        .unwrap()
                };

                set_layouts.push(set_layout);
                set_layout_info.push(Default::default());
            }
        }

        (set_layouts, set_layout_info)
    }

    pub fn from_file(path: &str, kind: ShaderKind, entry_point: &str) -> Self {
        let compiler = shaderc::Compiler::new().unwrap();
        let mut options = shaderc::CompileOptions::new().unwrap();
        options.add_macro_definition("EP", Some("main"));
        options.set_target_env(
            shaderc::TargetEnv::Vulkan,
            shaderc::EnvVersion::Vulkan1_2 as u32,
        );
        options.set_generate_debug_info();

        let spirv = compiler
            .compile_into_spirv(
                &std::fs::read_to_string(path).unwrap(),
                kind.to_shaderc_kind(),
                path,
                entry_point,
                Some(&options),
            )
            .unwrap();

        Self::new(spirv.as_binary_u8(), kind, entry_point)
    }
}