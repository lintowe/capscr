use anyhow::{anyhow, Result};
use image::RgbaImage;
use xcap::Window;

use super::Capture;
#[cfg(test)]
use super::WindowInfo;

pub struct WindowCapture {
    window_id: u32,
}

impl WindowCapture {
    pub fn new(window_id: u32) -> Self {
        Self { window_id }
    }

    #[cfg(test)]
    pub fn from_title(title: &str) -> Result<Self> {
        let windows = Window::all()?;
        let window = windows
            .into_iter()
            .find(|w| w.title().contains(title))
            .ok_or_else(|| anyhow!("Window with title '{}' not found", title))?;
        Ok(Self {
            window_id: window.id(),
        })
    }

    #[cfg(test)]
    pub fn focused() -> Result<Self> {
        let windows = Window::all()?;
        let window = windows
            .into_iter()
            .find(|w| !w.is_minimized() && !w.title().is_empty())
            .ok_or_else(|| anyhow!("No focused window found"))?;
        Ok(Self {
            window_id: window.id(),
        })
    }

    fn find_window(&self) -> Result<Window> {
        let windows = Window::all()?;
        windows
            .into_iter()
            .find(|w| w.id() == self.window_id)
            .ok_or_else(|| anyhow!("Window {} not found", self.window_id))
    }

    #[cfg(test)]
    pub fn list_application_windows() -> Result<Vec<WindowInfo>> {
        let windows = Window::all()?;
        let mut app_windows: Vec<WindowInfo> = windows
            .into_iter()
            .filter(|w| {
                !w.title().is_empty()
                    && w.width() > 50
                    && w.height() > 50
                    && !w.is_minimized()
            })
            .map(|w| WindowInfo {
                id: w.id(),
                title: w.title().to_string(),
                app_name: w.app_name().to_string(),
                x: w.x(),
                y: w.y(),
                width: w.width(),
                height: w.height(),
            })
            .collect();

        app_windows.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(app_windows)
    }
}

impl Capture for WindowCapture {
    fn capture(&self) -> Result<RgbaImage> {
        let window = self.find_window()?;
        let img = window.capture_image()?;
        Ok(img)
    }
}
