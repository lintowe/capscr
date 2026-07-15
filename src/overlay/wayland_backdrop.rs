use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output::WlOutput, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{delegate_noop, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer},
    zwlr_layer_surface_v1::{self, Anchor},
};

pub struct BackdropFrame {
    pub output_name: String,
    pub image: Arc<RgbaImage>,
}

pub struct NativeBackdrop {
    shutdown: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl NativeBackdrop {
    pub fn show(frames: Vec<BackdropFrame>) -> Result<Self> {
        let (ready_tx, ready_rx) = channel();
        let (shutdown_tx, shutdown_rx) = channel();
        let thread = std::thread::Builder::new()
            .name("capscr-wayland-backdrop".into())
            .spawn(move || {
                if let Err(error) = run(frames, shutdown_rx, &ready_tx) {
                    tracing::warn!("native wayland backdrop unavailable: {error:#}");
                    let _ = ready_tx.send(Err(error));
                }
            })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                shutdown: shutdown_tx,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(anyhow!("native wayland backdrop stopped before mapping"))
            }
        }
    }
}

impl Drop for NativeBackdrop {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct BackdropState {
    configured: HashSet<String>,
}

delegate_noop!(BackdropState: ignore wl_compositor::WlCompositor);
delegate_noop!(BackdropState: ignore wl_shm::WlShm);
delegate_noop!(BackdropState: ignore wl_shm_pool::WlShmPool);
delegate_noop!(BackdropState: ignore wl_buffer::WlBuffer);
delegate_noop!(BackdropState: ignore wl_surface::WlSurface);
delegate_noop!(BackdropState: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(BackdropState: ignore wp_viewporter::WpViewporter);
delegate_noop!(BackdropState: ignore wp_viewport::WpViewport);

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, String> for BackdropState {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        output_name: &String,
        _connection: &wayland_client::Connection,
        _queue: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                surface.ack_configure(serial);
                state.configured.insert(output_name.clone());
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.configured.remove(output_name);
            }
            _ => {}
        }
    }
}

struct Surface {
    wl_surface: wl_surface::WlSurface,
    layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    _viewport: wp_viewport::WpViewport,
    _pool: wl_shm_pool::WlShmPool,
    _buffer: wl_buffer::WlBuffer,
    _file: File,
}

fn run(
    frames: Vec<BackdropFrame>,
    shutdown: std::sync::mpsc::Receiver<()>,
    ready: &Sender<Result<()>>,
) -> Result<()> {
    let wayshot = libwayshot_xcap::WayshotConnection::new()?;
    let mut event_queue = wayshot.conn.new_event_queue::<BackdropState>();
    let queue = event_queue.handle();
    let compositor =
        wayshot
            .globals
            .bind::<wl_compositor::WlCompositor, _, _>(&queue, 1..=6, ())?;
    let shm = wayshot
        .globals
        .bind::<wl_shm::WlShm, _, _>(&queue, 1..=1, ())?;
    let layer_shell = wayshot
        .globals
        .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&queue, 1..=5, ())?;
    let viewporter =
        wayshot
            .globals
            .bind::<wp_viewporter::WpViewporter, _, _>(&queue, 1..=1, ())?;
    let mut state = BackdropState {
        configured: HashSet::new(),
    };
    let mut surfaces = Vec::with_capacity(frames.len());

    for frame in frames {
        let output = wayshot
            .get_all_outputs()
            .iter()
            .find(|output| output.name == frame.output_name)
            .ok_or_else(|| anyhow!("wayland output {} disappeared", frame.output_name))?;
        surfaces.push(map_surface(
            &queue,
            &mut event_queue,
            &mut state,
            &compositor,
            &shm,
            &layer_shell,
            &viewporter,
            &output.wl_output,
            &frame,
            output.logical_region.inner.size.width,
            output.logical_region.inner.size.height,
        )?);
    }
    tracing::info!(
        "mapped {} native wayland selector backdrops",
        surfaces.len()
    );
    let _ = ready.send(Ok(()));

    // static backdrops need no redraws after every output commits its native buffer
    let _ = shutdown.recv();
    for surface in &surfaces {
        surface.wl_surface.attach(None, 0, 0);
        surface.wl_surface.commit();
        surface.layer_surface.destroy();
    }
    event_queue.roundtrip(&mut state)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn map_surface(
    queue: &QueueHandle<BackdropState>,
    event_queue: &mut wayland_client::EventQueue<BackdropState>,
    state: &mut BackdropState,
    compositor: &wl_compositor::WlCompositor,
    shm: &wl_shm::WlShm,
    layer_shell: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
    viewporter: &wp_viewporter::WpViewporter,
    output: &WlOutput,
    frame: &BackdropFrame,
    logical_width: u32,
    logical_height: u32,
) -> Result<Surface> {
    let width = frame.image.width();
    let height = frame.image.height();
    let stride = width.checked_mul(4).context("selector frame is too wide")?;
    let size = stride
        .checked_mul(height)
        .context("selector frame is too large")?;
    let (mut file, path) = create_shm_file(frame.output_name.as_str(), size)?;
    let mut bgra = vec![0; size as usize];
    crate::capture::par_convert(frame.image.as_raw(), &mut bgra, |pixel| {
        [pixel[2], pixel[1], pixel[0], 255]
    });
    file.write_all(&bgra)?;
    file.seek(SeekFrom::Start(0))?;
    let _ = std::fs::remove_file(path);

    let pool = shm.create_pool(file.as_fd(), size as i32, queue, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        wl_shm::Format::Argb8888,
        queue,
        (),
    );
    let surface = compositor.create_surface(queue, ());
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        Some(output),
        Layer::Top,
        "capscr-selector-backdrop".into(),
        queue,
        frame.output_name.clone(),
    );
    layer_surface.set_exclusive_zone(-1);
    layer_surface.set_anchor(Anchor::Top | Anchor::Right | Anchor::Bottom | Anchor::Left);
    layer_surface.set_size(0, 0);
    surface.commit();
    while !state.configured.contains(&frame.output_name) {
        event_queue.blocking_dispatch(state)?;
    }

    let viewport = viewporter.get_viewport(&surface, queue, ());
    viewport.set_destination(logical_width as i32, logical_height as i32);
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, width as i32, height as i32);
    surface.commit();
    event_queue.roundtrip(state)?;

    Ok(Surface {
        wl_surface: surface,
        layer_surface,
        _viewport: viewport,
        _pool: pool,
        _buffer: buffer,
        _file: file,
    })
}

fn create_shm_file(output_name: &str, size: u32) -> Result<(File, PathBuf)> {
    for nonce in 0..32 {
        let path = PathBuf::from(format!(
            "/dev/shm/capscr-selector-{}-{}-{nonce}",
            std::process::id(),
            output_name.replace('/', "_")
        ));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                file.set_len(size as u64)?;
                return Ok((file, path));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(anyhow!("couldn't allocate selector shared memory"))
}
