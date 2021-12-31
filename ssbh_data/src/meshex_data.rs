use itertools::Itertools;
use ssbh_lib::formats::meshex::AllData;
pub use ssbh_lib::{CString, Vector4};
use ssbh_lib::{MeshEx, Ptr64, Vector3};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::mesh_data::MeshObjectData;
use crate::SsbhData;

// TODO: Documentation.
#[cfg_attr(feature = "serde1", derive(Serialize, Deserialize))]
#[derive(Debug, PartialEq)]
pub struct MeshExData {
    pub mesh_object_groups: Vec<MeshObjectGroupData>,
}

#[cfg_attr(feature = "serde1", derive(Serialize, Deserialize))]
#[derive(Debug, PartialEq)]
pub struct MeshObjectGroupData {
    pub bounding_sphere: Vector4,
    pub mesh_object_full_name: String,
    pub mesh_object_name: String,
    // One entry for each mesh object?
    pub entry_flags: Vec<EntryFlags>,
}

#[cfg_attr(feature = "serde1", derive(Serialize, Deserialize))]
#[derive(Debug, PartialEq, Eq)]
pub struct EntryFlags {
    pub draw_model: bool,
    pub cast_shadow: bool,
    // TODO: Preserve remaining flags?
}

impl SsbhData for MeshExData {
    type WriteError = std::io::Error;

    fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(MeshEx::from_file(path)?.into())
    }

    fn read<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(MeshEx::read(reader)?.into())
    }

    fn write<W: std::io::Write + std::io::Seek>(
        &self,
        writer: &mut W,
    ) -> Result<(), Self::WriteError> {
        MeshEx::from(self).write(writer)
    }

    fn write_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Self::WriteError> {
        MeshEx::from(self).write_to_file(path)
    }
}

impl MeshExData {
    pub fn from_mesh_objects(objects: &[MeshObjectData]) -> Self {
        // TODO: Calculate proper bounding spheres?
        // TODO: Should flags always default to true?
        Self {
            mesh_object_groups: objects
                .iter()
                .group_by(|o| &o.name)
                .into_iter()
                .map(|(name, group)| MeshObjectGroupData {
                    bounding_sphere: Vector4::ZERO,
                    mesh_object_full_name: name.clone(),
                    mesh_object_name: strip_mesh_name_tags(name),
                    entry_flags: group
                        .into_iter()
                        .map(|_| EntryFlags {
                            draw_model: true,
                            cast_shadow: true,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl MeshObjectGroupData {
    fn from_points() -> Self {
        todo!()
    }
}

fn strip_mesh_name_tags(full_name: &str) -> String {
    // Strip portions of a mesh object's name that aren't necessary for identification.
    // This includes Autodesk Maya's convention of appending "Shape".
    // Names can contain multiple tags like "face_default_O_V_VISShape" -> "face_default".
    let vis_index = full_name.find("_VIS");
    let o_index = full_name.find("_O_");
    match (vis_index, o_index) {
        (None, None) => {
            // Handle the special case where the name only contains shape.
            if full_name.ends_with("Shape") {
                full_name
                    .rfind("Shape")
                    .map(|end_index| full_name.get(..end_index))
                    .flatten()
                    .unwrap_or(full_name)
                    .to_string()
            } else {
                full_name.to_string()
            }
        }
        _ => {
            // Unwrap first since we don't want None < Some.
            let end_index = std::cmp::min(
                vis_index.unwrap_or(full_name.len()),
                o_index.unwrap_or(full_name.len()),
            );
            full_name.get(..end_index).unwrap_or(full_name).to_string()
        }
    }
}

// We shouldn't have any errors here?
// TODO: Do we care about null pointers?
// TODO: How will we calculate the file length?
// Just buffer into a vec and use the length?
impl From<MeshEx> for MeshExData {
    fn from(m: MeshEx) -> Self {
        Self::from(&m)
    }
}

impl From<&MeshEx> for MeshExData {
    fn from(m: &MeshEx) -> Self {
        Self {
            mesh_object_groups: m
                .mesh_object_groups
                .as_ref()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, g)| MeshObjectGroupData {
                    bounding_sphere: g.bounding_sphere,
                    mesh_object_full_name: g
                        .mesh_object_full_name
                        .as_ref()
                        .unwrap()
                        .to_string_lossy(),
                    mesh_object_name: g.mesh_object_name.as_ref().unwrap().to_string_lossy(),
                    entry_flags: m
                        .entries
                        .as_ref()
                        .unwrap()
                        .iter()
                        .positions(|e| e.mesh_object_group_index as usize == i)
                        .map(|entry_index| {
                            // TODO: Error handling here for invalid indices?
                            let entry_flags = m.entry_flags.as_ref().unwrap().0[entry_index];
                            EntryFlags {
                                draw_model: entry_flags.draw_model(),
                                cast_shadow: entry_flags.cast_shadow(),
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl From<MeshExData> for MeshEx {
    fn from(m: MeshExData) -> Self {
        Self::from(&m)
    }
}

impl From<&MeshExData> for MeshEx {
    fn from(m: &MeshExData) -> Self {
        Self {
            // TODO: How do we calculate length without a writer?
            // Is there some sort of heuristic for calculating it?
            file_length: 0,
            entry_count: m
                .mesh_object_groups
                .iter()
                .map(|g| g.entry_flags.len() as u32)
                .sum(),
            mesh_object_group_count: m.mesh_object_groups.len() as u32,
            all_data: Ptr64::new(AllData {
                // TODO: Calculate the correct bounding sphere.
                bounding_sphere: Vector4::ZERO,
                name: Ptr64::new("All".into()),
            }),
            mesh_object_groups: Ptr64::new(
                m.mesh_object_groups
                    .iter()
                    .map(|g| ssbh_lib::formats::meshex::MeshObjectGroup {
                        bounding_sphere: g.bounding_sphere,
                        mesh_object_full_name: Ptr64::new(g.mesh_object_full_name.as_str().into()),
                        mesh_object_name: Ptr64::new(g.mesh_object_name.as_str().into()),
                    })
                    .collect(),
            ),
            entries: Ptr64::new(
                m.mesh_object_groups
                    .iter()
                    .enumerate()
                    .map(|(i, g)| {
                        g.entry_flags
                            .iter()
                            .map(move |_| ssbh_lib::formats::meshex::MeshEntry {
                                mesh_object_group_index: i as u32,
                                unk1: Vector3::new(0.0, 1.0, 0.0),
                            })
                    })
                    .flatten()
                    .collect(),
            ),
            entry_flags: Ptr64::new(ssbh_lib::formats::meshex::EntryFlags(
                m.mesh_object_groups
                    .iter()
                    .map(|g| {
                        g.entry_flags.iter().map(|e| {
                            ssbh_lib::formats::meshex::EntryFlag::new()
                                .with_draw_model(e.draw_model)
                                .with_cast_shadow(e.cast_shadow)
                        })
                    })
                    .flatten()
                    .collect(),
            )),
            unk1: 0, // TODO: Preserve this value?
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::mesh_data::{AttributeData, VectorData};

    use super::*;

    use ssbh_lib::{
        formats::meshex::{AllData, MeshEntry, MeshObjectGroup},
        MeshEx, Ptr64, Vector3, Vector4,
    };

    #[test]
    fn convert_meshex_data() {
        // TODO: Test a case with valid indices.
        let meshex = MeshEx {
            file_length: 0,
            entry_count: 2,
            mesh_object_group_count: 2,
            all_data: Ptr64::new(AllData {
                bounding_sphere: Vector4::ZERO,
                name: Ptr64::new("All".into()),
            }),
            mesh_object_groups: Ptr64::new(vec![
                MeshObjectGroup {
                    bounding_sphere: Vector4::ZERO,
                    mesh_object_full_name: Ptr64::new("a_VIS".into()),
                    mesh_object_name: Ptr64::new("a".into()),
                },
                MeshObjectGroup {
                    bounding_sphere: Vector4::ZERO,
                    mesh_object_full_name: Ptr64::new("b_VIS".into()),
                    mesh_object_name: Ptr64::new("b".into()),
                },
            ]),
            entries: Ptr64::new(vec![
                MeshEntry {
                    mesh_object_group_index: 0,
                    unk1: Vector3::new(0.0, 1.0, 0.0),
                },
                MeshEntry {
                    mesh_object_group_index: 0,
                    unk1: Vector3::new(0.0, 1.0, 0.0),
                },
                MeshEntry {
                    mesh_object_group_index: 1,
                    unk1: Vector3::new(0.0, 1.0, 0.0),
                },
            ]),
            entry_flags: Ptr64::new(ssbh_lib::formats::meshex::EntryFlags(vec![
                ssbh_lib::formats::meshex::EntryFlag::new()
                    .with_draw_model(false)
                    .with_cast_shadow(true),
                ssbh_lib::formats::meshex::EntryFlag::new()
                    .with_draw_model(true)
                    .with_cast_shadow(false),
                ssbh_lib::formats::meshex::EntryFlag::new()
                    .with_draw_model(true)
                    .with_cast_shadow(true),
            ])),
            unk1: 0,
        };

        let data = MeshExData {
            mesh_object_groups: vec![
                MeshObjectGroupData {
                    bounding_sphere: Vector4::ZERO,
                    mesh_object_full_name: "a_VIS".to_string(),
                    mesh_object_name: "a".to_string(),
                    entry_flags: vec![
                        EntryFlags {
                            draw_model: false,
                            cast_shadow: true,
                        },
                        EntryFlags {
                            draw_model: true,
                            cast_shadow: false,
                        },
                    ],
                },
                MeshObjectGroupData {
                    bounding_sphere: Vector4::ZERO,
                    mesh_object_full_name: "b_VIS".to_string(),
                    mesh_object_name: "b".to_string(),
                    entry_flags: vec![EntryFlags {
                        draw_model: true,
                        cast_shadow: true,
                    }],
                },
            ],
        };

        assert_eq!(data, MeshExData::from(&meshex));

        let new_meshex = MeshEx::from(&data);
        // TODO: How to test file length?
        // TODO: Test bounding spheres?
        assert_eq!(3, new_meshex.entry_count);
        assert_eq!(2, new_meshex.mesh_object_group_count);
        assert_eq!(
            "All",
            new_meshex
                .all_data
                .as_ref()
                .unwrap()
                .name
                .as_ref()
                .unwrap()
                .to_string_lossy()
        );

        let group = &new_meshex.mesh_object_groups.as_ref().unwrap()[0];
        assert_eq!(
            "a",
            group.mesh_object_name.as_ref().unwrap().to_string_lossy()
        );
        assert_eq!(
            "a_VIS",
            group
                .mesh_object_full_name
                .as_ref()
                .unwrap()
                .to_string_lossy()
        );

        let group = &new_meshex.mesh_object_groups.as_ref().unwrap()[1];
        assert_eq!(
            "b",
            group.mesh_object_name.as_ref().unwrap().to_string_lossy()
        );
        assert_eq!(
            "b_VIS",
            group
                .mesh_object_full_name
                .as_ref()
                .unwrap()
                .to_string_lossy()
        );

        assert_eq!(
            0,
            new_meshex.entries.as_ref().unwrap()[0].mesh_object_group_index
        );
        assert_eq!(
            Vector3::new(0.0, 1.0, 0.0),
            new_meshex.entries.as_ref().unwrap()[0].unk1
        );

        assert_eq!(
            0,
            new_meshex.entries.as_ref().unwrap()[1].mesh_object_group_index
        );
        assert_eq!(
            Vector3::new(0.0, 1.0, 0.0),
            new_meshex.entries.as_ref().unwrap()[1].unk1
        );

        assert_eq!(
            1,
            new_meshex.entries.as_ref().unwrap()[2].mesh_object_group_index
        );
        assert_eq!(
            Vector3::new(0.0, 1.0, 0.0),
            new_meshex.entries.as_ref().unwrap()[2].unk1
        );

        assert_eq!(
            ssbh_lib::formats::meshex::EntryFlag::new()
                .with_draw_model(false)
                .with_cast_shadow(true),
            new_meshex.entry_flags.as_ref().unwrap().0[0]
        );
        assert_eq!(
            ssbh_lib::formats::meshex::EntryFlag::new()
                .with_draw_model(true)
                .with_cast_shadow(false),
            new_meshex.entry_flags.as_ref().unwrap().0[1]
        );
        assert_eq!(
            ssbh_lib::formats::meshex::EntryFlag::new()
                .with_draw_model(true)
                .with_cast_shadow(true),
            new_meshex.entry_flags.as_ref().unwrap().0[2]
        );
    }

    #[test]
    fn meshex_data_from_mesh_objects() {
        // TODO: This should test the bounding spheres.
        let data = MeshExData {
            mesh_object_groups: vec![
                MeshObjectGroupData {
                    bounding_sphere: Vector4::ZERO,
                    mesh_object_full_name: "a_VIS".to_string(),
                    mesh_object_name: "a".to_string(),
                    entry_flags: vec![
                        EntryFlags {
                            draw_model: true,
                            cast_shadow: true,
                        },
                        EntryFlags {
                            draw_model: true,
                            cast_shadow: true,
                        },
                    ],
                },
                MeshObjectGroupData {
                    bounding_sphere: Vector4::ZERO,
                    mesh_object_full_name: "b_VIS".to_string(),
                    mesh_object_name: "b".to_string(),
                    entry_flags: vec![EntryFlags {
                        draw_model: true,
                        cast_shadow: true,
                    }],
                },
            ],
        };

        // TODO: Implement Default for MeshObjectData?
        assert_eq!(
            data,
            MeshExData::from_mesh_objects(&[
                MeshObjectData {
                    name: "a_VIS".to_string(),
                    sub_index: 0,
                    parent_bone_name: String::new(),
                    sort_bias: 0,
                    disable_depth_write: false,
                    disable_depth_test: false,
                    vertex_indices: Vec::new(),
                    positions: vec![AttributeData {
                        name: String::new(),
                        data: VectorData::Vector3(vec![[0.0, 0.0, 0.0]; 3])
                    }],
                    normals: Vec::new(),
                    binormals: Vec::new(),
                    tangents: Vec::new(),
                    texture_coordinates: Vec::new(),
                    color_sets: Vec::new(),
                    bone_influences: Vec::new(),
                },
                MeshObjectData {
                    name: "a_VIS".to_string(),
                    sub_index: 0,
                    parent_bone_name: String::new(),
                    sort_bias: 0,
                    disable_depth_write: false,
                    disable_depth_test: false,
                    vertex_indices: Vec::new(),
                    positions: vec![AttributeData {
                        name: String::new(),
                        data: VectorData::Vector3(vec![[0.0, 0.0, 0.0]; 3])
                    }],
                    normals: Vec::new(),
                    binormals: Vec::new(),
                    tangents: Vec::new(),
                    texture_coordinates: Vec::new(),
                    color_sets: Vec::new(),
                    bone_influences: Vec::new(),
                },
                MeshObjectData {
                    name: "b_VIS".to_string(),
                    sub_index: 0,
                    parent_bone_name: String::new(),
                    sort_bias: 0,
                    disable_depth_write: false,
                    disable_depth_test: false,
                    vertex_indices: Vec::new(),
                    positions: vec![AttributeData {
                        name: String::new(),
                        data: VectorData::Vector3(vec![[0.0, 0.0, 0.0]; 3])
                    }],
                    normals: Vec::new(),
                    binormals: Vec::new(),
                    tangents: Vec::new(),
                    texture_coordinates: Vec::new(),
                    color_sets: Vec::new(),
                    bone_influences: Vec::new(),
                }
            ])
        );
    }

    #[test]
    fn strip_meshex_names() {
        // Generated from a dump of numshexb file entries.
        // Each test case is a unique removed tag found by comparing the name and full name.
        assert_eq!("sampleRing", strip_mesh_name_tags("sampleRingShape"));
        assert_eq!(
            "CityWorldFlag01_pCylinderShape1",
            strip_mesh_name_tags("CityWorldFlag01_pCylinderShape1")
        );
        assert_eq!(
            "Face_patternA",
            strip_mesh_name_tags("Face_patternA_VIS_O_OBJShape")
        );
        assert_eq!(
            "FaceBaseM",
            strip_mesh_name_tags("FaceBaseM_O_OBJ_NSCShape")
        );
        assert_eq!(
            "guile_base",
            strip_mesh_name_tags("guile_base_VIS_OBJShape")
        );
        assert_eq!("gel", strip_mesh_name_tags("gel_O_OBJ_O_SORTEACHNODEShape"));
        assert_eq!("FilmM3", strip_mesh_name_tags("FilmM3_O_OBJShape"));
        assert_eq!(
            "BodyM_Red",
            strip_mesh_name_tags("BodyM_Red_VIS_O_OBJ_NSCShape")
        );
        assert_eq!(
            "OrbM",
            strip_mesh_name_tags("OrbM_VIS_O_OBJ_O_OBJ_NSCShape")
        );
        assert_eq!("SheriffM", strip_mesh_name_tags("SheriffM_O_OBJ_NSC1Shape"));
        assert_eq!(
            "face_default",
            strip_mesh_name_tags("face_default_O_V_VISShape")
        );
        assert_eq!("a", strip_mesh_name_tags("a_O_OBJ_O_SORTEACHNODEShape"));
        assert_eq!("shotM", strip_mesh_name_tags("shotM_VIS_O_OBJ1Shape"));
        assert_eq!(
            "peach_flame_RT",
            strip_mesh_name_tags("peach_flame_RT_O_OBJ_O_HIR_O_SORTBIAS800_Shape")
        );
        assert_eq!(
            "ref",
            strip_mesh_name_tags("ref_O_OBJ_O_HIR_O_SORTBIAS900_Shape")
        );
        assert_eq!(
            "peach_00_hair2",
            strip_mesh_name_tags("peach_00_hair2_O_OBJ_O_HIR_O_SORTBIAS1000_Shape")
        );
        assert_eq!(
            "peach_00_skirt3",
            strip_mesh_name_tags("peach_00_skirt3_O_OBJ_O_HIR_O_SORTBIAS1100_Shape")
        );
        assert_eq!(
            "peach_00_main2",
            strip_mesh_name_tags("peach_00_main2_O_OBJ_O_HIR_O_SORTBIAS1200_Shape")
        );
        assert_eq!(
            "peach_00_shose",
            strip_mesh_name_tags("peach_00_shose_O_OBJ_O_HIR_O_SORTBIAS1300_Shape")
        );
        assert_eq!(
            "peach_00_skirt",
            strip_mesh_name_tags("peach_00_skirt_O_OBJ_O_HIR_O_SORTBIAS1400_Shape")
        );
        assert_eq!(
            "peach_00_hand",
            strip_mesh_name_tags("peach_00_hand_O_OBJ_O_HIR_O_SORTBIAS1500_Shape")
        );
        assert_eq!(
            "peach_01L_hair03",
            strip_mesh_name_tags("peach_01L_hair03_O_OBJ_O_HIR_O_SORTBIAS1600_Shape")
        );
        assert_eq!(
            "peach_00_hair4",
            strip_mesh_name_tags("peach_00_hair4_O_OBJ_O_HIR_O_SORTBIAS1800_Shape")
        );
        assert_eq!(
            "peach_01R_ring_R",
            strip_mesh_name_tags("peach_01R_ring_R_O_OBJ_O_HIR_O_SORTBIAS1700_Shape")
        );
        assert_eq!("eye1", strip_mesh_name_tags("eye1_O_OBJ_O_HIRShape"));
        assert_eq!(
            "renz",
            strip_mesh_name_tags("renz_O_OBJ_NSC_O_SORTBIASm10_Shape")
        );
        assert_eq!(
            "Bayonetta_FaceN",
            strip_mesh_name_tags("Bayonetta_FaceN_VIS_O_OBJ_O_NOSORT_FAR_O_SORTEACHNODEShape")
        );
        assert_eq!(
            "armA",
            strip_mesh_name_tags("armA_VIS_O_OBJ_O_SORTEACHNODEShape")
        );
        assert_eq!(
            "brave_Eye_Ouch",
            strip_mesh_name_tags("brave_Eye_Ouch_VIS_O_OBJShape_t_t")
        );
        assert_eq!(
            "brave_Mouth_Bound",
            strip_mesh_name_tags("brave_Mouth_Bound_VIS_O_OBJShape1")
        );
        assert_eq!(
            "falsh",
            strip_mesh_name_tags("falsh_O_OBJ_O_NOSORT_FARShape")
        );
        assert_eq!(
            "Cloud_Openblink",
            strip_mesh_name_tags("Cloud_Openblink_VIS_O_OBJ_O_NOSORT_FARShape")
        );
        assert_eq!(
            "L_ARMura",
            strip_mesh_name_tags("L_ARMura_O_OBJ_O_SORTBIASm1__O_SORTEACHNODEShape")
        );
        assert_eq!(
            "L_ARMomote",
            strip_mesh_name_tags("L_ARMomote_O_OBJ_O_SORTBIAS1__O_SORTEACHNODEShape")
        );
        assert_eq!(
            "diddy_Mouth_Capture",
            strip_mesh_name_tags("diddy_Mouth_Capture_VIS_O_OBJ_SORTEACHNODEShape")
        );
        assert_eq!(
            "Head_normalLF_Shadow",
            strip_mesh_name_tags("Head_normalLF_Shadow_VIS_O_OBJ_O_SORTBIAS5__O_SORTEACHNODEShape")
        );
        assert_eq!(
            "donkey_Rarm",
            strip_mesh_name_tags("donkey_Rarm_O_OBJ_O_SORTEACHNODEShape1")
        );
        assert_eq!(
            "bird",
            strip_mesh_name_tags("bird_VIS_O_OBJ_O_OBJ_O_NOSORT_NEARShape")
        );
        assert_eq!(
            "gun_A_board",
            strip_mesh_name_tags("gun_A_board_VIS_O_OBJShapeShape")
        );
        assert_eq!("LightM", strip_mesh_name_tags("LightM_O_OBJ_2Shape"));
        assert_eq!(
            "LhandMove",
            strip_mesh_name_tags("LhandMove_VIS_O_OBJ_O_NOSORT_AS_OPAQUEShape")
        );
        assert_eq!(
            "renz",
            strip_mesh_name_tags("renz_O_OBJ_NSC_O_SORTBIASm10_Shape1")
        );
        assert_eq!(
            "pasted__polySurface287",
            strip_mesh_name_tags("pasted__polySurface287_O_OBJ1Shape")
        );
        assert_eq!(
            "szerosuits_heir_Tail",
            strip_mesh_name_tags("szerosuits_heir_Tail_O_OBJ__O_SORTEACHNODEShape")
        );
        assert_eq!(
            "pPlane1",
            strip_mesh_name_tags("pPlane1_O_OBJ_NSC_O_SORTEACHNODEShape")
        );
        assert_eq!(
            "hairmid_rev",
            strip_mesh_name_tags("hairmid_rev_O_OBJ_O_SORTEACHNODEShape_rev")
        );
    }
}
