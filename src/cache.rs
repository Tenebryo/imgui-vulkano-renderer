use std::{collections::HashMap, sync::Arc};

use imgui::TextureId;
use vulkano::descriptor_set::PersistentDescriptorSet;

#[derive(Default)]
pub(crate) struct DescriptorSetCache {
    cache: HashMap<TextureId, Arc<PersistentDescriptorSet>>,

    font_texture: Option<Arc<PersistentDescriptorSet>>,
}

impl DescriptorSetCache {
    pub fn get_or_insert<F>(
        &mut self,
        texture_id: TextureId,
        creation_fn: F,
    ) -> Result<Arc<PersistentDescriptorSet>, Box<dyn std::error::Error>>
    where
        F: FnOnce(TextureId) -> Result<Arc<PersistentDescriptorSet>, Box<dyn std::error::Error>>,
    {
        if texture_id.id() == usize::MAX {
            if self.font_texture.is_none() {
                let set = creation_fn(texture_id)?;
                self.font_texture = Some(set);
            }
            Ok(Arc::clone(self.font_texture.as_ref().unwrap()))
        } else {
            use std::collections::hash_map::Entry::*;
            let entry = self.cache.entry(texture_id);
            match entry {
                Vacant(entry) => {
                    let set = creation_fn(texture_id)?;
                    Ok(Arc::clone(entry.insert(set)))
                }
                Occupied(entry) => Ok(Arc::clone(entry.get())),
            }
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn clear_font_texture(&mut self) {
        self.font_texture = None;
    }
}
