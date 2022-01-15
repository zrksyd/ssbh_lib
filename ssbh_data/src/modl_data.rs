//! Types for working with [Modl] data in .numdlb or .nusrcmdlb files.
//!
//! # Examples
//! [Modl] files assign materials to a model.
/*!
```rust no_run
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use ssbh_data::SsbhData;
use ssbh_data::modl_data::ModlData;

let modl = ModlData::from_file("model.numdlb")?;

for entry in modl.entries {
    println!(
        "Mesh: {}{}, Material: {}",
        entry.mesh_object_name, entry.mesh_object_sub_index, entry.material_label
    );
}
# Ok(()) }
```
 */
use std::{
    io::{Read, Seek},
    path::Path,
};

use ssbh_lib::{formats::modl::*, SsbhString, Version};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{create_ssbh_array, SsbhData};

/// The data associated with a [Modl] file.
/// The supported version is 1.7.
#[cfg_attr(feature = "serde1", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[derive(Debug)]
pub struct ModlData {
    pub major_version: u16,
    pub minor_version: u16,
    pub model_name: String,
    pub skeleton_file_name: String,
    pub material_file_names: Vec<String>,
    pub animation_file_name: Option<String>,
    pub mesh_file_name: String,
    pub entries: Vec<ModlEntryData>,
}

/// Data associated with a [ModlEntry].
#[cfg_attr(feature = "serde1", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[derive(Debug)]
pub struct ModlEntryData {
    pub mesh_object_name: String,
    pub mesh_object_sub_index: u64,
    pub material_label: String,
}

impl SsbhData for ModlData {
    type WriteError = std::io::Error;

    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let modl = Modl::from_file(path)?;
        Ok((&modl).into())
    }

    fn read<R: Read + Seek>(reader: &mut R) -> Result<Self, Box<dyn std::error::Error>> {
        let modl = Modl::read(reader)?;
        Ok((&modl).into())
    }

    fn write<W: std::io::Write + Seek>(&self, writer: &mut W) -> std::io::Result<()> {
        let modl: Modl = self.into();
        modl.write(writer)?;
        Ok(())
    }

    fn write_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let modl: Modl = self.into();
        modl.write_to_file(path)?;
        Ok(())
    }
}

// Define two way conversions between types.
impl From<Modl> for ModlData {
    fn from(m: Modl) -> Self {
        Self::from(&m)
    }
}

impl From<&Modl> for ModlData {
    fn from(m: &Modl) -> Self {
        let (major_version, minor_version) = m.major_minor_version();
        match m {
            Modl::V17 {
                model_name,
                skeleton_file_name,
                material_file_names,
                animation_file_name,
                mesh_file_name,
                entries,
            } => Self {
                major_version,
                minor_version,
                model_name: model_name.to_string_lossy(),
                skeleton_file_name: skeleton_file_name.to_string_lossy(),
                material_file_names: material_file_names
                    .elements
                    .iter()
                    .map(|f| f.to_string_lossy())
                    .collect(),
                animation_file_name: (*animation_file_name).as_ref().map(|s| s.to_string_lossy()),
                mesh_file_name: mesh_file_name.to_string_lossy(),
                entries: entries.elements.iter().map(|e| e.into()).collect(),
            },
        }
    }
}

impl From<ModlData> for Modl {
    fn from(m: ModlData) -> Self {
        Self::from(&m)
    }
}

impl From<&ModlData> for Modl {
    fn from(m: &ModlData) -> Self {
        Self::V17 {
            model_name: m.model_name.clone().into(),
            skeleton_file_name: m.skeleton_file_name.clone().into(),
            material_file_names: create_ssbh_array(&m.material_file_names, |f| f.as_str().into()),
            animation_file_name: m.animation_file_name.as_ref().map(SsbhString::from).into(),
            mesh_file_name: m.mesh_file_name.as_str().into(),
            entries: create_ssbh_array(&m.entries, |e| e.into()),
        }
    }
}

impl From<ModlEntryData> for ModlEntry {
    fn from(m: ModlEntryData) -> Self {
        Self::from(&m)
    }
}

impl From<&ModlEntryData> for ModlEntry {
    fn from(m: &ModlEntryData) -> Self {
        Self {
            mesh_object_name: m.mesh_object_name.as_str().into(),
            mesh_object_sub_index: m.mesh_object_sub_index,
            material_label: m.material_label.as_str().into(),
        }
    }
}

impl From<&ModlEntry> for ModlEntryData {
    fn from(m: &ModlEntry) -> Self {
        Self {
            mesh_object_name: m.mesh_object_name.to_string_lossy(),
            mesh_object_sub_index: m.mesh_object_sub_index,
            material_label: m.material_label.to_string_lossy(),
        }
    }
}

impl From<ModlEntry> for ModlEntryData {
    fn from(m: ModlEntry) -> Self {
        Self::from(&m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssbh_lib::SsbhString;

    #[test]
    fn create_modl() {
        let data = ModlData {
            major_version: 1,
            minor_version: 2,
            model_name: "a".into(),
            skeleton_file_name: "b".into(),
            material_file_names: vec!["f1".into(), "f2".into()],
            animation_file_name: Some("c".into()),
            mesh_file_name: "d".into(),
            entries: vec![ModlEntryData {
                mesh_object_name: "a".into(),
                mesh_object_sub_index: 2,
                material_label: "b".into(),
            }],
        };

        let ssbh: Modl = data.into();
        match ssbh {
            Modl::V17 {
                model_name,
                skeleton_file_name,
                material_file_names,
                animation_file_name,
                mesh_file_name,
                entries,
            } => {
                assert_eq!("a", model_name.to_str().unwrap());
                assert_eq!("b", skeleton_file_name.to_str().unwrap());
                assert_eq!("f1", material_file_names.elements[0].to_str().unwrap());
                assert_eq!("f2", material_file_names.elements[1].to_str().unwrap());
                let s = match &(*animation_file_name) {
                    Some(s) => s,
                    None => panic!(),
                };
                assert_eq!("c", s.to_str().unwrap());
                assert_eq!("d", mesh_file_name.to_str().unwrap());
                assert_eq!("a", entries.elements[0].mesh_object_name.to_str().unwrap());
                assert_eq!(2, entries.elements[0].mesh_object_sub_index);
                assert_eq!("b", entries.elements[0].material_label.to_str().unwrap());
            }
        }
    }

    #[test]
    fn create_modl_data() {
        let ssbh = Modl::V17 {
            model_name: "a".into(),
            skeleton_file_name: "b".into(),
            material_file_names: vec![SsbhString::from("f1"), SsbhString::from("f2")].into(),
            animation_file_name: Some("c".into()).into(),
            mesh_file_name: "d".into(),
            entries: vec![ModlEntry {
                mesh_object_name: "a".into(),
                mesh_object_sub_index: 2,
                material_label: "b".into(),
            }]
            .into(),
        };

        let data: ModlData = ssbh.into();
        assert_eq!(1, data.major_version);
        assert_eq!(7, data.minor_version);
        assert_eq!("a", data.model_name);
        assert_eq!(vec!["f1", "f2"], data.material_file_names);
        assert_eq!("c", data.animation_file_name.unwrap());
        assert_eq!("a", data.entries[0].mesh_object_name);
        assert_eq!(2, data.entries[0].mesh_object_sub_index);
        assert_eq!("b", data.entries[0].material_label);
    }

    #[test]
    fn create_modl_entry_data() {
        let ssbh = ModlEntry {
            mesh_object_name: "a".into(),
            mesh_object_sub_index: 2,
            material_label: "b".into(),
        };

        let data: ModlEntryData = ssbh.into();
        assert_eq!("a", data.mesh_object_name);
        assert_eq!(2, data.mesh_object_sub_index);
        assert_eq!("b", data.material_label);
    }

    #[test]
    fn create_modl_entry() {
        let data = ModlEntryData {
            mesh_object_name: "a".into(),
            mesh_object_sub_index: 2,
            material_label: "b".into(),
        };

        let ssbh: ModlEntry = data.into();
        assert_eq!("a", ssbh.mesh_object_name.to_str().unwrap());
        assert_eq!(2, ssbh.mesh_object_sub_index);
        assert_eq!("b", ssbh.material_label.to_str().unwrap());
    }
}
