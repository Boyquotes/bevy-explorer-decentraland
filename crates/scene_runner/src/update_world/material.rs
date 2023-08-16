use bevy::{ecs::system::SystemParam, pbr::NotShadowCaster, prelude::*, render::primitives::Aabb};

use crate::{renderer_context::RendererSceneContext, ContainerEntity, SceneSets};
use common::util::TryInsertEx;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        common::{texture_union, TextureUnion},
        sdk::components::{pb_material, MaterialTransparencyMode, PbMaterial},
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::IpfsLoaderExt;

use super::AddCrdtInterfaceExt;

pub struct MaterialDefinitionPlugin;

#[derive(Component, Debug, Default, Clone)]
pub struct MaterialDefinition {
    pub material: StandardMaterial,
    pub shadow_caster: bool,
    pub base_color_texture: Option<TextureUnion>,
    pub emmissive_texture: Option<TextureUnion>,
    pub normal_map: Option<TextureUnion>,
}

impl From<PbMaterial> for MaterialDefinition {
    fn from(value: PbMaterial) -> Self {
        let (material, base_color_texture, emmissive_texture, normal_map) = match &value.material {
            Some(pb_material::Material::Unlit(unlit)) => {
                let base_color = unlit.diffuse_color.map(Color::from).unwrap_or(Color::WHITE);

                let alpha_mode = if base_color.a() < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                };

                (
                    StandardMaterial {
                        base_color,
                        double_sided: true,
                        cull_mode: None,
                        unlit: true,
                        alpha_mode,
                        ..Default::default()
                    },
                    unlit.texture.clone(),
                    None,
                    None,
                )
            }
            Some(pb_material::Material::Pbr(pbr)) => {
                let base_color = pbr.albedo_color.map(Color::from).unwrap_or(Color::WHITE);

                let alpha_mode = match pbr
                    .transparency_mode
                    .map(MaterialTransparencyMode::from_i32)
                    .unwrap_or(None)
                {
                    Some(MaterialTransparencyMode::MtmOpaque) => AlphaMode::Opaque,
                    Some(MaterialTransparencyMode::MtmAlphaTest) => {
                        AlphaMode::Mask(pbr.alpha_test.unwrap_or(0.5))
                    }
                    Some(MaterialTransparencyMode::MtmAlphaBlend) => AlphaMode::Blend,
                    Some(MaterialTransparencyMode::MtmAlphaTestAndAlphaBlend) => unimplemented!(), // TODO requires bevy patch or custom material or material extension tbd
                    Some(MaterialTransparencyMode::MtmAuto) | None => {
                        if base_color.a() < 1.0 {
                            AlphaMode::Blend
                        } else {
                            AlphaMode::Opaque
                        }
                    }
                };

                let emissive_intensity = pbr.emissive_intensity.unwrap_or(2.0);
                let emissive = if let Some(color) = pbr.emissive_color {
                    Color::from(color) * emissive_intensity
                } else if pbr.emissive_texture.is_some() {
                    Color::WHITE * emissive_intensity
                } else {
                    Color::BLACK
                };

                (
                    StandardMaterial {
                        base_color,
                        emissive,
                        // TODO what is pbr.reflectivity_color?
                        metallic: pbr.metallic.unwrap_or(0.5),
                        perceptual_roughness: pbr.roughness.unwrap_or(0.5),
                        // TODO specular intensity
                        double_sided: true,
                        cull_mode: None,
                        alpha_mode,
                        ..Default::default()
                    },
                    pbr.texture.clone(),
                    pbr.emissive_texture.clone(),
                    pbr.bump_texture.clone(),
                )
            }
            None => Default::default(),
        };

        let shadow_caster = match value.material {
            Some(pb_material::Material::Unlit(unlit)) => unlit.cast_shadows,
            Some(pb_material::Material::Pbr(pbr)) => pbr.cast_shadows,
            _ => None,
        }
        .unwrap_or(true);

        Self {
            material,
            shadow_caster,
            base_color_texture,
            emmissive_texture,
            normal_map,
        }
    }
}

impl Plugin for MaterialDefinitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbMaterial, MaterialDefinition>(
            SceneComponentId::MATERIAL,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            (update_materials, update_bias).in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component)]
pub struct RetryMaterial;

#[derive(Component)]
pub struct TouchMaterial;

#[derive(Component)]
pub struct VideoTextureOutput(pub Handle<Image>);

pub enum TextureResolveError {
    SourceNotAvailable,
    SourceNotReady,
    SceneNotFound,
    NotImplemented,
}

#[derive(SystemParam)]
pub struct TextureResolver<'w, 's> {
    scenes: Query<'w, 's, &'static RendererSceneContext>,
    asset_server: Res<'w, AssetServer>,
    videos: Query<'w, 's, &'static VideoTextureOutput>,
}

#[derive(Debug)]
pub struct ResolvedTexture {
    pub image: Handle<Image>,
    pub touch: bool,
}

impl<'w, 's> TextureResolver<'w, 's> {
    pub fn resolve_texture(
        &self,
        scene: Entity,
        texture: &texture_union::Tex,
    ) -> Result<ResolvedTexture, TextureResolveError> {
        let Ok(scene) = self.scenes.get(scene) else {
            return Err(TextureResolveError::SceneNotFound);
        };

        match texture {
            texture_union::Tex::Texture(texture) => {
                // TODO handle wrapmode and filtering once we have some asset processing pipeline in place (bevy 0.11-0.12)
                Ok(ResolvedTexture {
                    image: self
                        .asset_server
                        .load_content_file::<Image>(&texture.src, &scene.hash)
                        .unwrap(),
                    touch: false,
                })
            }
            texture_union::Tex::AvatarTexture(_) => Err(TextureResolveError::NotImplemented),
            texture_union::Tex::VideoTexture(vt) => {
                let Some(video_entity) = scene.bevy_entity(SceneEntityId::from_proto_u32(vt.video_player_entity)) else {
                    warn!("failed to look up video source entity");
                    return Err(TextureResolveError::SourceNotAvailable);
                };

                if let Ok(vt) = self.videos.get(video_entity) {
                    debug!("adding video texture {:?}", vt.0);
                    Ok(ResolvedTexture {
                        image: vt.0.clone(),
                        touch: true,
                    })
                } else {
                    warn!("video source entity not ready, retrying ...");
                    Err(TextureResolveError::SourceNotReady)
                }
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_materials(
    mut commands: Commands,
    mut new_materials: Query<
        (Entity, &MaterialDefinition, &ContainerEntity),
        Or<(Changed<MaterialDefinition>, With<RetryMaterial>)>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    touch: Query<&Handle<StandardMaterial>, With<TouchMaterial>>,
    resolver: TextureResolver,
) {
    for (ent, defn, container) in new_materials.iter_mut() {
        let textures: Result<Vec<_>, _> = [
            &defn.base_color_texture,
            &defn.emmissive_texture,
            &defn.normal_map,
        ]
        .into_iter()
        .map(
            |texture| match texture.as_ref().and_then(|t| t.tex.as_ref()) {
                Some(texture) => match resolver.resolve_texture(container.root, texture) {
                    Ok(resolved) => Ok(Some(resolved)),
                    Err(TextureResolveError::SourceNotReady) => Err(()),
                    Err(_) => Ok(None),
                },
                None => Ok(None),
            },
        )
        .collect();

        let textures = match textures {
            Ok(textures) => textures,
            _ => {
                commands.entity(ent).insert(RetryMaterial);
                continue;
            }
        };

        if textures
            .iter()
            .any(|t| t.as_ref().map_or(false, |t| t.touch))
        {
            commands.entity(ent).insert(TouchMaterial);
        }

        let [base_color_texture, emissive_texture, normal_map_texture]: [Option<ResolvedTexture>;
            3] = textures.try_into().unwrap();

        let mut commands = commands.entity(ent);
        commands
            .remove::<RetryMaterial>()
            .try_insert(materials.add(StandardMaterial {
                base_color_texture: base_color_texture.map(|t| t.image),
                emissive_texture: emissive_texture.map(|t| t.image),
                normal_map_texture: normal_map_texture.map(|t| t.image),
                ..defn.material.clone()
            }));
        if defn.shadow_caster {
            commands.remove::<NotShadowCaster>();
        } else {
            commands.try_insert(NotShadowCaster);
        }
    }

    for touch in touch.iter() {
        materials.get_mut(touch);
    }
}

#[allow(clippy::type_complexity)]
fn update_bias(
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<
        (&Aabb, &Handle<StandardMaterial>),
        Or<(Changed<Handle<StandardMaterial>>, Changed<Aabb>)>,
    >,
) {
    for (aabb, h_material) in query.iter() {
        if let Some(material) = materials.get_mut(h_material) {
            if material.alpha_mode == AlphaMode::Blend {
                // add a bias based on the aabb size, to force an explicit transparent order which is
                // hopefully correct, but should be better than nothing even if not always perfect
                material.depth_bias = aabb.half_extents.length() * 1e-5;
            }
        }
    }
}
