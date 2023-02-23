use color_eyre::eyre::Result;
use glam::Quat;
use manifest_dir_macros::directory_relative_path;
use stardust_xr_fusion::{
	client::{Client, FrameInfo, RootHandler},
	core::values::Transform,
	drawable::{MaterialParameter, Model, ResourceID},
	fields::BoxField,
	items::{
		panel::{self, PanelItem, PanelItemHandler, PanelItemInitData, ToplevelInfo},
		Item, ItemAcceptor, ItemAcceptorHandler,
	},
	node::NodeError,
	spatial::Spatial,
	HandlerWrapper, Mutex,
};
use stardust_xr_molecules::keyboard::{KeyboardPanelHandler, KeyboardPanelRelay};
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
	_item: PanelItem,
	toplevel_info: Option<ToplevelInfo>,
	_keyboard_relay: KeyboardPanelRelay,
}

type AcceptorHandler = HandlerWrapper<ItemAcceptor<PanelItem>, Poltergeist>;
struct Poltergeist {
	_self_ref: Weak<Mutex<Poltergeist>>,
	root: Spatial,
	bound_field: BoxField,
	model: Model,
	captured: Option<CapturedItem>,
}
impl Poltergeist {
	fn new(client: &Arc<Client>) -> Result<(Arc<Mutex<Self>>, AcceptorHandler), NodeError> {
		let root = Spatial::create(client.get_root(), Transform::default(), true)?;
		let field = BoxField::create(
			&root,
			Transform::from_position([0.0, 0.227573, -0.084663]),
			[0.471, 0.46, 0.168],
		)?;
		let model = Model::create(
			&root,
			Transform::default(),
			&ResourceID::new_namespaced("poltergeist", "crt"),
		)?;
		let acceptor = ItemAcceptor::create(&root, Transform::default(), &field)?;
		let poltergeist = Arc::new_cyclic(|weak| {
			Mutex::new(Poltergeist {
				_self_ref: weak.clone(),
				root,
				bound_field: field,
				model,
				captured: None,
			})
		});
		Ok((poltergeist.clone(), acceptor.wrap_raw(poltergeist)?))
	}
}
impl RootHandler for Poltergeist {
	fn frame(&mut self, _info: FrameInfo) {}
}
const SCREEN_MATERIAL_INDEX: u32 = 3;
impl ItemAcceptorHandler<PanelItem> for Poltergeist {
	fn captured(&mut self, uid: &str, item: PanelItem, init_data: PanelItemInitData) {
		if self.captured.is_some() {
			println!("Already captured something into Poltergeist");
			let _ = item.release();
			return;
		}
		println!("Captured {uid} into Poltergeist");

		let _ = item.set_transform(
			Some(&self.root),
			Transform::from_position_rotation_scale([0.0, -0.25, -0.4], Quat::IDENTITY, [1.0; 3]),
		);
		let _ = item.configure_toplevel(
			Some([640, 480].into()),
			&[panel::State::Activated, panel::State::Maximized],
			None,
		);
		let _ = item.apply_toplevel_material(&self.model, SCREEN_MATERIAL_INDEX);
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
		)
		.unwrap();
		self.captured.replace(CapturedItem {
			uid: uid.to_string(),
			_item: item,
			toplevel_info: init_data.toplevel,
			_keyboard_relay: keyboard_relay,
		});
	}
	fn released(&mut self, uid: &str) {
		if self.captured.is_some() && self.captured.as_ref().unwrap().uid == uid {
			self.captured.take();
		}
	}
}
impl PanelItemHandler for Poltergeist {
	fn commit_toplevel(&mut self, state: Option<ToplevelInfo>) {
		self.captured.as_mut().unwrap().toplevel_info = state;
	}
}
