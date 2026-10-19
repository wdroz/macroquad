use std::collections::HashSet;

/// Source material behavior beyond the opt-in renderer's default preview.
///
/// These diagnostics do not select lighting, PBR, refraction, or shadows. Alpha
/// modes, sidedness, base-color/emissive textures and their supported UV mappings
/// are handled by the renderer and do not produce these warnings.
///
/// ```
/// use macroquad::gltf_draw::GltfUnsupportedMaterialFeature;
/// let feature = GltfUnsupportedMaterialFeature::Lighting;
/// assert!(matches!(feature, GltfUnsupportedMaterialFeature::Lighting));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GltfUnsupportedMaterialFeature {
    /// A material without `KHR_materials_unlit` expects glTF's lighting model.
    /// The renderer defaults to base-plus-emission preview. Opting into
    /// [`super::GltfLighting`] adds diffuse hemisphere/directional lighting.
    /// [`super::GltfMaterialFrame::submit_shaded`] also samples normal and
    /// metallic-roughness maps. Environment reflections, occlusion and shadows
    /// are still omitted, so this warning remains.
    Lighting,
    /// Previously reported a normal map the renderer could not sample.
    /// Retained so existing matches still compile. The lit renderer samples
    /// the map when it is submitted with the chunk, and this variant is no
    /// longer produced.
    NormalTexture,
    /// Previously reported a metallic-roughness map the renderer could not sample.
    /// Retained so existing matches still compile. The lit renderer samples
    /// the map when it is submitted with the chunk, and this variant is no
    /// longer produced.
    MetallicRoughnessTexture,
    /// An occlusion map is not loaded or sampled by this renderer.
    OcclusionTexture,
    /// `KHR_materials_transmission` is not evaluated; no refraction is applied.
    Transmission,
    /// The source primitive has `COLOR_0`, which the existing model loader does
    /// not retain. Its vertex colors therefore cannot modulate the base color.
    VertexColors,
    /// An optional material extension is not implemented. The string is its
    /// exact extension name, for example `KHR_materials_clearcoat`.
    Extension(String),
}

/// One omitted feature at a particular source node's primitive/material use.
///
/// Repeated mesh instances receive separate warnings so callers can identify
/// each affected path. A chunked primitive receives one warning per feature,
/// rather than one warning for every converted mesh chunk. Indices are local to
/// the inspected source file; they are not identifiers shared by different GLBs.
///
/// ```
/// use macroquad::gltf_draw::GltfMaterialWarning;
/// fn affected_paths(warnings: &[GltfMaterialWarning]) -> Vec<&str> {
///     warnings.iter().map(|warning| warning.path.as_str()).collect()
/// }
/// assert!(affected_paths(&[]).is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GltfMaterialWarning {
    /// Preorder node index, matching [`super::GltfDrawMesh::node`] after loading
    /// the same source successfully.
    pub node: usize,
    /// Node index in the original GLB's JSON nodes array.
    pub source_node: usize,
    /// Node name, falling back to the mesh name, then `"unnamed"`, matching
    /// [`super::GltfDrawMesh::name`].
    pub name: String,
    /// Ancestor names and this node's name joined with `/`, matching
    /// [`super::GltfDrawMesh::path`]. Names containing `/` are not escaped.
    pub path: String,
    /// Primitive index within the node's original mesh.
    pub primitive: usize,
    /// Index in the source materials array, or `None` for glTF's default material.
    pub material_index: Option<usize>,
    /// Optional source material name.
    pub material_name: Option<String>,
    /// Behavior omitted by the renderer.
    pub feature: GltfUnsupportedMaterialFeature,
    /// Source image index for an omitted core texture slot, when applicable.
    pub image_index: Option<usize>,
    /// Optional name of that source image.
    pub image_name: Option<String>,
}

/// Inspects a GLB for material behavior omitted by the opt-in renderer.
///
/// This is an explicit, CPU-only diagnostic: it does not log, decode images or
/// geometry arrays, upload resources, or require a running macroquad window.
/// It uses the model loader's validated document parser and its default-scene
/// selection (falling back to the first scene). Only material uses in that
/// scene are reported. The order is node preorder, then primitive and feature.
/// Unused materials and scenes produce no warnings. A repeated node or cycle
/// is an error, matching the model loader's scene-graph policy.
///
/// Every material without `KHR_materials_unlit`, including the default material,
/// produces [`GltfUnsupportedMaterialFeature::Lighting`]. This source inspection
/// does not know the frame's lighting choice: the default base-plus-emissive
/// preview omits the metallic-roughness response. Opting into
/// [`super::GltfLighting`] and submitting material maps evaluates tangent-space
/// normals and a directional metallic-roughness term, still without
/// environment lighting, occlusion, or shadows. For `KHR_materials_unlit`, base color and alpha
/// define the appearance; emissive, normal, occlusion and metallic-roughness
/// properties are ignored as required by that extension, without warnings.
/// Optional material extensions other than unlit and emissive strength are
/// reported separately. No light configuration, refraction, or shadow system
/// is implied by these diagnostics.
///
/// A GLB binary chunk is required, and unsupported *required* extensions remain
/// document errors, as in the model loader. This inspection does not validate
/// geometry payloads, image contents, or supported texture UV compatibility;
/// successful inspection does not guarantee successful model loading. Skin,
/// animation and morph-target support are outside these material diagnostics.
/// Warnings describe the source; later application edits to loaded surfaces or
/// textures are not reflected here.
///
/// ```no_run
/// use macroquad::gltf_draw::gltf_material_warnings;
/// # fn inspect(bytes: &[u8]) -> Result<(), String> {
/// for warning in gltf_material_warnings(bytes)? {
///     println!("{}: material {:?}, {:?}", warning.path,
///         warning.material_name, warning.feature);
/// }
/// # Ok(())
/// # }
/// ```
pub fn gltf_material_warnings(bytes: &[u8]) -> Result<Vec<GltfMaterialWarning>, String> {
    let (document, _binary) = super::parse_glb_document(bytes)?;
    let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    else {
        return Ok(Vec::new());
    };

    let mut pending: Vec<_> = scene.nodes().map(|node| (node, None::<String>)).collect();
    pending.reverse();
    let mut seen = HashSet::new();
    let mut warnings = Vec::new();
    while let Some((node, parent_path)) = pending.pop() {
        let node_index = seen.len();
        if !seen.insert(node.index()) {
            return Err(format!(
                "glTF node {} appears more than once in the scene graph",
                node.index()
            ));
        }
        let mesh = node.mesh();
        let name = node
            .name()
            .or_else(|| mesh.as_ref().and_then(gltf::Mesh::name))
            .unwrap_or("unnamed");
        let path = parent_path.map_or_else(|| name.to_owned(), |p| format!("{p}/{name}"));
        if let Some(mesh) = mesh {
            for primitive in mesh.primitives() {
                let material = primitive.material();
                let context = GltfMaterialWarning {
                    node: node_index,
                    source_node: node.index(),
                    name: name.to_owned(),
                    path: path.clone(),
                    primitive: primitive.index(),
                    material_index: material.index(),
                    material_name: material.name().map(str::to_owned),
                    feature: GltfUnsupportedMaterialFeature::Lighting,
                    image_index: None,
                    image_name: None,
                };
                let mut add = |feature, texture: Option<gltf::Texture<'_>>| {
                    let image = texture.map(|texture| texture.source());
                    warnings.push(GltfMaterialWarning {
                        feature,
                        image_index: image.as_ref().map(gltf::Image::index),
                        image_name: image.and_then(|image| image.name().map(str::to_owned)),
                        ..context.clone()
                    });
                };
                use GltfUnsupportedMaterialFeature as F;
                if primitive.get(&gltf::Semantic::Colors(0)).is_some() {
                    add(F::VertexColors, None);
                }
                if material.extension_value("KHR_materials_unlit").is_none() {
                    add(F::Lighting, None);
                    if let Some(info) = material.occlusion_texture() {
                        add(F::OcclusionTexture, Some(info.texture()));
                    }
                }
                if let Some(extensions) = material.extensions() {
                    let mut names: Vec<_> = extensions.keys().collect();
                    names.sort_unstable();
                    for name in names {
                        match name.as_str() {
                            "KHR_materials_unlit" | "KHR_materials_emissive_strength" => {}
                            "KHR_materials_transmission" => add(F::Transmission, None),
                            _ => add(F::Extension(name.clone()), None),
                        }
                    }
                }
            }
        }
        let children: Vec<_> = node.children().collect();
        pending.extend(
            children
                .into_iter()
                .rev()
                .map(|node| (node, Some(path.clone()))),
        );
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use serde_json::{json, Value};

    fn fixture(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut json = json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": 36 }],
            "bufferViews": [{ "buffer": 0, "byteLength": 36 }],
            "accessors": [{ "bufferView": 0, "componentType": 5126, "count": 3,
                "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0] }],
            "materials": [{ "name": "paint" }],
            "meshes": [{ "name": "triangle", "primitives": [
                { "attributes": { "POSITION": 0 }, "material": 0 }
            ] }],
            "nodes": [{ "name": "part", "mesh": 0 }],
            "scenes": [{ "nodes": [0] }]
        });
        edit(&mut json);
        let positions: [f32; 9] = [0., 0., 0., 1., 0., 0., 0., 1., 0.];
        let binary: Vec<_> = positions.into_iter().flat_map(f32::to_le_bytes).collect();
        gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(serde_json::to_vec(&json).unwrap()),
            bin: Some(Cow::Owned(binary)),
        }
        .to_vec()
        .unwrap()
    }

    #[test]
    fn warnings_match_loaded_preorder_and_report_each_instance() {
        let bytes = fixture(|j| {
            j["nodes"] = json!([
                { "name": "unused", "mesh": 0 },
                { "name": "root", "children": [3, 2] },
                { "mesh": 0 },
                { "name": "child", "mesh": 0 }
            ]);
            j["scenes"] = json!([{ "nodes": [0] }, { "nodes": [1] }]);
            j["scene"] = json!(1);
        });
        let warnings = gltf_material_warnings(&bytes).unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!((warnings[0].node, warnings[0].source_node), (1, 3));
        assert_eq!((warnings[1].node, warnings[1].source_node), (2, 2));
        assert_eq!(warnings[0].name, "child");
        assert_eq!(warnings[0].path, "root/child");
        assert_eq!(warnings[1].name, "triangle");
        assert_eq!(warnings[1].path, "root/triangle");
        assert_eq!(warnings[0].material_index, Some(0));
        assert_eq!(warnings[0].material_name.as_deref(), Some("paint"));
        assert_eq!(warnings[0].primitive, 0);
        assert_eq!(
            warnings[0].feature,
            GltfUnsupportedMaterialFeature::Lighting
        );
        let (scene, _, _) = super::super::prepare_glb(&bytes).unwrap();
        assert_eq!(scene.nodes[warnings[0].node].name.as_deref(), Some("child"));
    }

    #[test]
    fn omitted_texture_slots_include_image_identifiers_without_decoding() {
        let bytes = fixture(|j| {
            // Deliberately missing external files: ignored maps are not decoded.
            j["images"] = json!([
                { "name": "normal pixels", "uri": "missing-normal.png" },
                { "name": "roughness pixels", "uri": "missing-roughness.png" },
                { "uri": "missing-occlusion.png" }
            ]);
            j["textures"] = json!([{ "source": 0 }, { "source": 1 }, { "source": 2 }]);
            j["materials"][0]["normalTexture"] = json!({ "index": 0 });
            j["materials"][0]["pbrMetallicRoughness"] =
                json!({ "metallicRoughnessTexture": { "index": 1 } });
            j["materials"][0]["occlusionTexture"] = json!({ "index": 2 });
        });
        let warnings = gltf_material_warnings(&bytes).unwrap();
        use GltfUnsupportedMaterialFeature as F;
        assert_eq!(
            warnings.iter().map(|w| &w.feature).collect::<Vec<_>>(),
            [&F::Lighting, &F::OcclusionTexture]
        );
        assert_eq!(warnings[1].image_index, Some(2));
        assert_eq!(warnings[1].image_name, None);
        assert!(warnings.iter().all(|warning| !matches!(
            warning.feature,
            F::NormalTexture | F::MetallicRoughnessTexture
        )));
    }

    #[test]
    fn unlit_intentionally_ignores_core_lighting_and_emissive_properties() {
        let bytes = fixture(|j| {
            j["images"] = json!([{ "uri": "missing.png" }]);
            j["textures"] = json!([{ "source": 0 }]);
            j["materials"][0]["normalTexture"] = json!({ "index": 0 });
            j["materials"][0]["occlusionTexture"] = json!({ "index": 0 });
            j["materials"][0]["pbrMetallicRoughness"] =
                json!({ "metallicFactor": 0.3, "metallicRoughnessTexture": { "index": 0 } });
            j["materials"][0]["emissiveTexture"] = json!({ "index": 0 });
            j["materials"][0]["emissiveFactor"] = json!([1, 0.5, 0.3]);
            j["materials"][0]["extensions"] = json!({
                "KHR_materials_unlit": {},
                "KHR_materials_emissive_strength": { "emissiveStrength": 2 }
            });
            j["extensionsRequired"] =
                json!(["KHR_materials_unlit", "KHR_materials_emissive_strength"]);
            j["extensionsUsed"] = j["extensionsRequired"].clone();
        });
        assert!(gltf_material_warnings(&bytes).unwrap().is_empty());
    }

    #[test]
    fn extensions_and_vertex_colors_are_structured_and_unused_materials_are_quiet() {
        let bytes = fixture(|j| {
            j["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = json!(0);
            j["materials"][0]["extensions"] = json!({
                "KHR_materials_transmission": { "transmissionFactor": 0.7 },
                "KHR_materials_clearcoat": { "clearcoatFactor": 1 },
                "KHR_materials_emissive_strength": { "emissiveStrength": 4 }
            });
            j["materials"].as_array_mut().unwrap().push(json!({
                "name": "unused", "extensions": { "UNUSED_material_extension": {} }
            }));
        });
        let warnings = gltf_material_warnings(&bytes).unwrap();
        use GltfUnsupportedMaterialFeature as F;
        assert_eq!(
            warnings
                .iter()
                .map(|w| w.feature.clone())
                .collect::<Vec<_>>(),
            [
                F::VertexColors,
                F::Lighting,
                F::Extension("KHR_materials_clearcoat".into()),
                F::Transmission
            ]
        );
        assert!(warnings.iter().all(|w| w.material_index == Some(0)));
    }

    #[test]
    fn default_material_first_scene_and_absent_scenes_follow_loader_selection() {
        let bytes = fixture(|j| {
            j["meshes"][0]["primitives"][0]
                .as_object_mut()
                .unwrap()
                .remove("material");
            j["scenes"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "nodes": [] }));
        });
        let warnings = gltf_material_warnings(&bytes).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].material_index, None);
        assert_eq!(warnings[0].material_name, None);
        let bytes = fixture(|j| {
            j.as_object_mut().unwrap().remove("scenes");
        });
        assert!(gltf_material_warnings(&bytes).unwrap().is_empty());
    }

    #[test]
    fn repeated_nodes_and_cycles_return_errors_without_unbounded_traversal() {
        for bytes in [
            fixture(|j| {
                j["scenes"][0]["nodes"] = json!([0, 0]);
            }),
            fixture(|j| {
                j["nodes"][0]["children"] = json!([0]);
            }),
        ] {
            assert!(gltf_material_warnings(&bytes)
                .unwrap_err()
                .contains("appears more than once"));
        }
    }

    #[test]
    fn required_unknown_extensions_and_invalid_documents_remain_errors() {
        let bytes = fixture(|j| {
            j["extensionsRequired"] = json!(["UNSUPPORTED_material_extension"]);
            j["extensionsUsed"] = j["extensionsRequired"].clone();
        });
        assert!(gltf_material_warnings(&bytes).is_err());
        assert!(gltf_material_warnings(b"invalid GLB").is_err());
        let bytes = fixture(|j| {
            j["nodes"][0]["mesh"] = json!(100);
        });
        assert!(gltf_material_warnings(&bytes).is_err());
    }
}
