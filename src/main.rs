use color_eyre::eyre::Result;
use glam::Quat;
use input_event_codes::BTN_LEFT;
use manifest_dir_macros::directory_relative_path;
use mint::Vector2;
use stardust_xr_fusion::{
	client::{Client, FrameInfo, RootHandler},
	core::values::Transform,
	drawable::{MaterialParameter, Model, ResourceID},
	fields::BoxField,
	items::{
		panel::{ChildInfo, Geometry, PanelItem, PanelItemHandler, PanelItemInitData, SurfaceID},
		Item, ItemAcceptor, ItemAcceptorHandler,
	},
	node::NodeError,
	spatial::Spatial,
	HandlerWrapper, Mutex,
};
use stardust_xr_molecules::{
	keyboard::{create_keyboard_panel_handler, KeyboardPanelHandler},
	touch_plane::TouchPlane,
};
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	color_eyre::install()?;
	let (client, event_loop) = Client::connect_with_async_loop().await?;
	client.set_base_prefixes(&[directory_relative_path!("res")]);

	let (poltergeist, _item_acceptor_handler) = Poltergeist::new(&client)?;
	client.wrap_root_raw(&poltergeist)?;

	tokio::select! {
		_ = tokio::signal::ctrl_c() => (),
		e = event_loop => e??,
	}
	Ok(())
}

struct CapturedItem {
	uid: String,
	size_tx: mpsc::Sender<Vector2<u32>>,
	_keyboard_relay: KeyboardPanelHandler,
}

type AcceptorHandler = HandlerWrapper<ItemAcceptor<PanelItem>, Poltergeist>;
struct Poltergeist {
	root: Spatial,
	bound_field: BoxField,
	model: Model,
	captured: Option<HandlerWrapper<PanelItem, CapturedItem>>,
	touch_plane: TouchPlane,
	size_tx: mpsc::Sender<Vector2<u32>>,
	size_rx: mpsc::Receiver<Vector2<u32>>,
}
const TOUCH_PLANE_WIDTH: f32 = 0.403122;
const TOUCH_PLANE_HEIGHT: f32 = 0.313059;
impl Poltergeist {
	fn new(client: &Arc<Client>) -> Result<(Arc<Mutex<Self>>, AcceptorHandler), NodeError> {
		let root = Spatial::create(client.get_root(), Transform::default(), true)?;
		let bound_field = BoxField::create(
			&root,
			Transform::from_position([0.0, 0.227573, -0.084663]),
			[0.471, 0.46, 0.168],
		)?;
		let model = Model::create(
			&root,
			Transform::default(),
			&ResourceID::new_namespaced("poltergeist", "crt"),
		)?;
		let acceptor = ItemAcceptor::create(&root, Transform::default(), &bound_field)?;
		let touch_plane = TouchPlane::create(
			&root,
			Transform::from_position([0.0, 0.268524, 0.0]),
			[TOUCH_PLANE_WIDTH, TOUCH_PLANE_HEIGHT],
			0.172038,
			0.0..1.0,
			0.0..1.0,
		)?;
		// touch_plane.input_handler().set_transform(None, Transform::from_position_scale([TOUCH_PLANE_WIDTH * -0.5, TOUCH_PLANE_HEIGHT * 0.5, 0.0], [TOUCH_PLANE_WIDTH, TOUCH_PLANE_HEIGHT,]))
		// touch_plane.set_debug(Some(DebugSettings::default()));
		let (size_tx, size_rx) = mpsc::channel(2);
		let poltergeist = Arc::new(Mutex::new(Poltergeist {
			root,
			bound_field,
			model,
			captured: None,
			touch_plane,
			size_tx,
			size_rx,
		}));
		Ok((poltergeist.clone(), acceptor.wrap_raw(poltergeist)?))
	}
}
impl RootHandler for Poltergeist {
	fn frame(&mut self, _info: FrameInfo) {
		self.touch_plane.update();
		while let Ok(size) = self.size_rx.try_recv() {
			self.touch_plane.x_range = 0.0..size.x as f32;
			self.touch_plane.y_range = 0.0..size.y as f32;
		}

		let Some(captured_item) = self.captured.as_mut() else {return};

		let touch_point = self.touch_plane.hover_points().first().cloned();
		if let Some(touch_point) = touch_point {
			// dbg!(touch_point);
			let _ = captured_item
				.node()
				.pointer_motion(&SurfaceID::Toplevel, touch_point);
		}
		if self.touch_plane.touch_started() {
			println!("touch started");
			let _ = captured_item
				.node()
				.pointer_button(&SurfaceID::Toplevel, BTN_LEFT!(), true);
		}
		if self.touch_plane.touch_stopped() {
			println!("touch stopped");
			let _ = captured_item
				.node()
				.pointer_button(&SurfaceID::Toplevel, BTN_LEFT!(), false);
		}
	}
}
impl ItemAcceptorHandler<PanelItem> for Poltergeist {
	fn captured(&mut self, uid: &str, item: PanelItem, _init_data: PanelItemInitData) {
		if let Some(captured) = self.captured.take() {
			let _ = captured.node().release();
		}
		println!("Captured {uid} into Poltergeist");

		let _ = item.set_transform(
			Some(&self.root),
			Transform::from_position_rotation_scale(
				[0.0, 0.268524, 0.05],
				Quat::IDENTITY,
				[1.0; 3],
			),
		);
		let _ = item.set_toplevel_size([640, 480].into());
		let screen = self.model.model_part("Screen").unwrap();
		let _ = item.apply_surface_material(&SurfaceID::Toplevel, &screen);
		let _ = screen.set_material_parameter("alpha_min", MaterialParameter::Float(1.0));

		let _keyboard_relay = create_keyboard_panel_handler(
			&self.root,
			Transform::from_position([0.070582, 0.052994, 0.000832]),
			&self.bound_field,
			&item,
			SurfaceID::Toplevel,
		)
		.unwrap();
		self.captured.replace(
			item.wrap(CapturedItem {
				uid: uid.to_string(),
				size_tx: self.size_tx.clone(),
				_keyboard_relay,
			})
			.unwrap(),
		);
	}
	fn released(&mut self, uid: &str) {
		if self.captured.is_some() && self.captured.as_ref().unwrap().lock_wrapped().uid == uid {
			self.captured.take();
		}
	}
}
impl PanelItemHandler for CapturedItem {
	fn toplevel_size_changed(&mut self, size: mint::Vector2<u32>) {
		let _ = self.size_tx.try_send(size);
	}

	fn new_child(&mut self, _uid: &str, _info: ChildInfo) {}
	fn reposition_child(&mut self, _uid: &str, _geometry: Geometry) {}
	fn drop_child(&mut self, _uid: &str) {}
}
