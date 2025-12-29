#![allow(dead_code)]

mod manifest;
mod loader;
mod wasm_runtime;

pub use manifest::{PluginManifest, PluginType};
pub use loader::PluginLoader;
pub use wasm_runtime::WasmPlugin;

use image::RgbaImage;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum PluginEvent {
    PreCapture {
        mode: CaptureType,
    },
    PostCapture {
        image: Arc<RgbaImage>,
        mode: CaptureType,
    },
    PreSave {
        image: Arc<RgbaImage>,
        path: PathBuf,
    },
    PostSave {
        path: PathBuf,
    },
    PreUpload {
        image: Arc<RgbaImage>,
    },
    PostUpload {
        url: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureType {
    FullScreen,
    Window,
    Region,
    Gif,
}

#[derive(Debug, Clone)]
pub enum PluginResponse {
    Continue,
    ModifiedImage(Arc<RgbaImage>),
    Cancel,
}

pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn description(&self) -> &str;

    fn on_event(&mut self, event: &PluginEvent) -> PluginResponse {
        let _ = event;
        PluginResponse::Continue
    }

    fn on_load(&mut self) {}
    fn on_unload(&mut self) {}
}

pub type CreatePluginFn = fn() -> Box<dyn Plugin>;

pub enum PluginHandle {
    Native {
        plugin: Box<dyn Plugin>,
        _library: libloading::Library,
    },
    Wasm {
        plugin: WasmPlugin,
    },
}

pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub handle: PluginHandle,
}

pub struct PluginManager {
    plugins: Vec<LoadedPlugin>,
    enabled: bool,
    plugins_dir: PathBuf,
}

impl PluginManager {
    pub fn new() -> Self {
        let plugins_dir = directories::ProjectDirs::from("", "", "capscr")
            .map(|d| d.config_dir().join("plugins"))
            .unwrap_or_else(|| PathBuf::from("plugins"));

        Self {
            plugins: Vec::new(),
            enabled: true,
            plugins_dir,
        }
    }

    pub fn with_plugins_dir(plugins_dir: PathBuf) -> Self {
        Self {
            plugins: Vec::new(),
            enabled: true,
            plugins_dir,
        }
    }

    pub fn plugins_dir(&self) -> &PathBuf {
        &self.plugins_dir
    }

    pub fn load_all(&mut self) -> Vec<String> {
        let mut errors = Vec::new();

        if !self.plugins_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&self.plugins_dir) {
                errors.push(format!("Failed to create plugins directory: {}", e));
                return errors;
            }
        }

        let entries = match std::fs::read_dir(&self.plugins_dir) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Failed to read plugins directory: {}", e));
                return errors;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                match self.load_from_directory(&path) {
                    Ok(()) => {}
                    Err(e) => errors.push(format!("{}: {}", path.display(), e)),
                }
            } else if path.extension().is_some_and(|ext| ext == "zip") {
                match self.install_from_zip(&path) {
                    Ok(()) => {}
                    Err(e) => errors.push(format!("{}: {}", path.display(), e)),
                }
            }
        }

        errors
    }

    pub fn install_from_zip(&mut self, zip_path: &PathBuf) -> Result<(), String> {
        let loader = PluginLoader::new(self.plugins_dir.clone());
        let plugin_dir = loader.install_from_zip(zip_path)?;
        self.load_from_directory(&plugin_dir)
    }

    pub fn load_from_directory(&mut self, dir: &Path) -> Result<(), String> {
        let loader = PluginLoader::new(self.plugins_dir.clone());
        let mut loaded = loader.load_from_directory(dir)?;

        match &mut loaded.handle {
            PluginHandle::Native { plugin, .. } => plugin.on_load(),
            PluginHandle::Wasm { plugin } => plugin.on_load(),
        }

        self.plugins.push(loaded);
        Ok(())
    }

    pub fn unload(&mut self, plugin_id: &str) -> bool {
        if let Some(pos) = self.plugins.iter().position(|p| p.manifest.plugin.id == plugin_id) {
            let mut loaded = self.plugins.remove(pos);
            match &mut loaded.handle {
                PluginHandle::Native { plugin, .. } => plugin.on_unload(),
                PluginHandle::Wasm { plugin } => plugin.on_unload(),
            }
            true
        } else {
            false
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn dispatch(&mut self, event: &PluginEvent) -> PluginResponse {
        if !self.enabled {
            return PluginResponse::Continue;
        }

        let mut current_image: Option<Arc<RgbaImage>> = None;

        for loaded in &mut self.plugins {
            let response = match &mut loaded.handle {
                PluginHandle::Native { plugin, .. } => plugin.on_event(event),
                PluginHandle::Wasm { plugin } => plugin.on_event(event),
            };
            match response {
                PluginResponse::Cancel => return PluginResponse::Cancel,
                PluginResponse::ModifiedImage(img) => {
                    current_image = Some(img);
                }
                PluginResponse::Continue => {}
            }
        }

        if let Some(img) = current_image {
            PluginResponse::ModifiedImage(img)
        } else {
            PluginResponse::Continue
        }
    }

    pub fn list(&self) -> Vec<&PluginManifest> {
        self.plugins.iter().map(|p| &p.manifest).collect()
    }

    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    pub fn get(&self, plugin_id: &str) -> Option<&PluginManifest> {
        self.plugins
            .iter()
            .find(|p| p.manifest.plugin.id == plugin_id)
            .map(|p| &p.manifest)
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        for loaded in &mut self.plugins {
            match &mut loaded.handle {
                PluginHandle::Native { plugin, .. } => plugin.on_unload(),
                PluginHandle::Wasm { plugin } => plugin.on_unload(),
            }
        }
    }
}
