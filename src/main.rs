pub mod delta;

use color_eyre::eyre::Result;
use delta::Delta;
use glam::Quat;
use input_event_codes::BTN_LEFT;
use manifest_dir_macros::directory_relative_path;
use stardust_xr_fusion::{
	client::{Client, FrameInfo, RootHandler},
	core::values::Transform,
	drawable::{MaterialParameter, Model, ResourceID},
	fields::BoxField,
	items::{
		panel::{self, PanelItem, PanelItemHandler, PanelItemInitData, SurfaceID, ToplevelInfo},
		Item, ItemAcceptor, ItemAcceptorHandler,
	},
	node::NodeError,
	spatial::Spatial,
	HandlerWrapper, Mutex,
};
use stardust_xr_molecules::{
	keyboard::{KeyboardPanelHandler, KeyboardPanelRelay},
	touch_plane::TouchPlane,
};
use std::sync::{Arc, Weak};

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
	toplevel_info: Delta<Option<ToplevelInfo>>,
	_keyboard_relay: KeyboardPanelRelay,
}

type AcceptorHandler = HandlerWrapper<ItemAcceptor<PanelItem>, Poltergeist>;
struct Poltergeist {
	_self_ref: Weak<Mutex<Poltergeist>>,
	root: Spatial,
	bound_field: BoxField,
	model: Model,
	captured: Option<HandlerWrapper<PanelItem, CapturedItem>>,
	touch_plane: TouchPlane,
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
		let poltergeist = Arc::new_cyclic(|weak| {
			Mutex::new(Poltergeist {
				_self_ref: weak.clone(),
				root,
				bound_field,
				model,
				captured: None,
				touch_plane,
			})
		});
		Ok((poltergeist.clone(), acceptor.wrap_raw(poltergeist)?))
	}
}
impl RootHandler for Poltergeist {
	fn frame(&mut self, _info: FrameInfo) {
		self.touch_plane.update();

		let Some(captured_item) = self.captured.as_mut() else {return};

		if let Some(delta) = captured_item.wrapped().lock().toplevel_info.delta() {
			if let Some(info) = delta {
				self.touch_plane.x_range = 0.0..info.size.x as f32;
				self.touch_plane.y_range = 0.0..info.size.y as f32;
			}
		}

		if self.touch_plane.touch_started() {
			println!("touch started");
			let _ = captured_item
				.node()
				.pointer_button(&SurfaceID::Toplevel, BTN_LEFT!(), true);
		}
		let touch_point = self.touch_plane.hover_points().first().cloned();
		if let Some(touch_point) = touch_point {
			// dbg!(toplevel_info.size);
			// dbg!(touch_point);
			let _ = captured_item
				.node()
				.pointer_motion(&SurfaceID::Toplevel, touch_point);
		}

		if self.touch_plane.touch_stopped() {
			println!("touch stopped");
			let _ = captured_item
				.node()
				.pointer_button(&SurfaceID::Toplevel, BTN_LEFT!(), false);
		}
	}
}
const SCREEN_MATERIAL_INDEX: u32 = 3;
impl ItemAcceptorHandler<PanelItem> for Poltergeist {
	fn captured(&mut self, uid: &str, item: PanelItem, init_data: PanelItemInitData) {
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
		let _ = item.configure_toplevel(
			Some([640, 480].into()),
			&[panel::State::Activated, panel::State::Maximized],
			None,
		);
		let _ =
			item.apply_surface_material(&SurfaceID::Toplevel, &self.model, SCREEN_MATERIAL_INDEX);
		let _ = self.model.set_material_parameter(
			SCREEN_MATERIAL_INDEX,
			"alpha_min",
			MaterialParameter::Float(1.0),
		);

		let keyboard_relay = KeyboardPanelHandler::create(
			&self.root,
			Transform::from_position([0.070582, 0.052994, 0.000832]),
			&self.bound_field,
			&item,
			SurfaceID::Toplevel,
		)
		.unwrap();
		let mut toplevel_info = Delta::new(init_data.toplevel);
		toplevel_info.mark_changed();
		self.captured.replace(
			item.wrap(CapturedItem {
				uid: uid.to_string(),
				toplevel_info,
				_keyboard_relay: keyboard_relay,
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
	fn commit_toplevel(&mut self, state: Option<ToplevelInfo>) {
		*self.toplevel_info.value_mut() = state;
	}
}
