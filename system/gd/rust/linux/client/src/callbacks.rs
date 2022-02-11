use crate::ClientContext;
use crate::{console_yellow, print_info};
use bt_topshim::btif::{BtBondState, BtSspVariant};
use bt_topshim::profiles::gatt::GattStatus;
use btstack::bluetooth::{
    BluetoothDevice, IBluetooth, IBluetoothCallback, IBluetoothConnectionCallback,
};
use btstack::bluetooth_gatt::{BluetoothGattService, IBluetoothGattCallback, LePhy};
use btstack::RPCProxy;
use manager_service::iface_bluetooth_manager::IBluetoothManagerCallback;
use std::sync::{Arc, Mutex};

/// Callback context for manager interface callbacks.
pub(crate) struct BtManagerCallback {
    objpath: String,
    context: Arc<Mutex<ClientContext>>,
}

impl BtManagerCallback {
    pub(crate) fn new(objpath: String, context: Arc<Mutex<ClientContext>>) -> Self {
        Self { objpath, context }
    }
}

impl IBluetoothManagerCallback for BtManagerCallback {
    fn on_hci_device_changed(&self, hci_interface: i32, present: bool) {
        print_info!("hci{} present = {}", hci_interface, present);

        if present {
            self.context.lock().unwrap().adapters.entry(hci_interface).or_insert(false);
        } else {
            self.context.lock().unwrap().adapters.remove(&hci_interface);
        }
    }

    fn on_hci_enabled_changed(&self, hci_interface: i32, enabled: bool) {
        self.context.lock().unwrap().set_adapter_enabled(hci_interface, enabled);
    }
}

impl manager_service::RPCProxy for BtManagerCallback {
    fn register_disconnect(&mut self, _f: Box<dyn Fn(u32) + Send>) -> u32 {
        0
    }

    fn get_object_id(&self) -> String {
        self.objpath.clone()
    }

    fn unregister(&mut self, _id: u32) -> bool {
        false
    }
}

/// Callback container for adapter interface callbacks.
pub(crate) struct BtCallback {
    objpath: String,
    context: Arc<Mutex<ClientContext>>,
}

impl BtCallback {
    pub(crate) fn new(objpath: String, context: Arc<Mutex<ClientContext>>) -> Self {
        Self { objpath, context }
    }
}

impl IBluetoothCallback for BtCallback {
    fn on_address_changed(&self, addr: String) {
        print_info!("Address changed to {}", &addr);
        self.context.lock().unwrap().adapter_address = Some(addr);
    }

    fn on_device_found(&self, remote_device: BluetoothDevice) {
        self.context
            .lock()
            .unwrap()
            .found_devices
            .entry(remote_device.address.clone())
            .or_insert(remote_device.clone());

        print_info!("Found device: {:?}", remote_device);
    }

    fn on_discovering_changed(&self, discovering: bool) {
        self.context.lock().unwrap().discovering_state = discovering;

        if discovering {
            self.context.lock().unwrap().found_devices.clear();
        }
        print_info!("Discovering: {}", discovering);
    }

    fn on_ssp_request(
        &self,
        remote_device: BluetoothDevice,
        _cod: u32,
        variant: BtSspVariant,
        passkey: u32,
    ) {
        match variant {
            BtSspVariant::PasskeyNotification => {
                print_info!(
                    "Device [{}: {}] would like to pair, enter passkey on remote device: {:06}",
                    &remote_device.address,
                    &remote_device.name,
                    passkey
                );
            }
            BtSspVariant::Consent => {
                let rd = remote_device.clone();
                self.context.lock().unwrap().run_callback(Box::new(move |context| {
                    // Auto-confirm bonding attempts that were locally initiated.
                    // Ignore all other bonding attempts.
                    let bonding_device = context.lock().unwrap().bonding_attempt.as_ref().cloned();
                    match bonding_device {
                        Some(bd) => {
                            if bd.address == rd.address {
                                context
                                    .lock()
                                    .unwrap()
                                    .adapter_dbus
                                    .as_ref()
                                    .unwrap()
                                    .set_pairing_confirmation(rd.clone(), true);
                            }
                        }
                        None => (),
                    }
                }));
            }
            BtSspVariant::PasskeyEntry => {
                println!("Got PasskeyEntry but it is not supported...");
            }
            BtSspVariant::PasskeyConfirmation => {
                println!("Got PasskeyConfirmation but there's nothing to do...");
            }
        }
    }

    fn on_bond_state_changed(&self, status: u32, address: String, state: u32) {
        print_info!("Bonding state changed: [{}] state: {}, Status = {}", address, state, status);

        // Clear bonding attempt if bonding fails or succeeds
        match BtBondState::from(state) {
            BtBondState::NotBonded | BtBondState::Bonded => {
                let bonding_attempt =
                    self.context.lock().unwrap().bonding_attempt.as_ref().cloned();
                match bonding_attempt {
                    Some(bd) => {
                        if &address == &bd.address {
                            self.context.lock().unwrap().bonding_attempt = None;
                        }
                    }
                    None => (),
                }
            }
            BtBondState::Bonding => (),
        }

        // If bonded, we should also automatically connect all enabled profiles
        if BtBondState::Bonded == state.into() {
            self.context.lock().unwrap().connect_all_enabled_profiles(BluetoothDevice {
                address,
                name: String::from("Classic device"),
            });
        }
    }
}

impl RPCProxy for BtCallback {
    fn register_disconnect(&mut self, _f: Box<dyn Fn(u32) + Send>) -> u32 {
        0
    }

    fn get_object_id(&self) -> String {
        self.objpath.clone()
    }

    fn unregister(&mut self, _id: u32) -> bool {
        false
    }
}

pub(crate) struct BtConnectionCallback {
    objpath: String,
    _context: Arc<Mutex<ClientContext>>,
}

impl BtConnectionCallback {
    pub(crate) fn new(objpath: String, _context: Arc<Mutex<ClientContext>>) -> Self {
        Self { objpath, _context }
    }
}

impl IBluetoothConnectionCallback for BtConnectionCallback {
    fn on_device_connected(&self, remote_device: BluetoothDevice) {
        print_info!("Connected: [{}]: {}", remote_device.address, remote_device.name);
    }

    fn on_device_disconnected(&self, remote_device: BluetoothDevice) {
        print_info!("Disconnected: [{}]: {}", remote_device.address, remote_device.name);
    }
}

impl RPCProxy for BtConnectionCallback {
    fn register_disconnect(&mut self, _f: Box<dyn Fn(u32) + Send>) -> u32 {
        0
    }

    fn get_object_id(&self) -> String {
        self.objpath.clone()
    }

    fn unregister(&mut self, _id: u32) -> bool {
        false
    }
}

pub(crate) struct BtGattCallback {
    objpath: String,
    context: Arc<Mutex<ClientContext>>,
}

impl BtGattCallback {
    pub(crate) fn new(objpath: String, context: Arc<Mutex<ClientContext>>) -> Self {
        Self { objpath, context }
    }
}

impl IBluetoothGattCallback for BtGattCallback {
    fn on_client_registered(&self, status: i32, client_id: i32) {
        print_info!("GATT Client registered status = {}, client_id = {}", status, client_id);
        self.context.lock().unwrap().gatt_client_id = Some(client_id);
    }

    fn on_client_connection_state(
        &self,
        status: i32,
        client_id: i32,
        connected: bool,
        addr: String,
    ) {
        print_info!(
            "GATT Client connection state = {}, client_id = {}, connected = {}, addr = {}",
            status,
            client_id,
            connected,
            addr
        );
    }

    fn on_phy_update(&self, addr: String, tx_phy: LePhy, rx_phy: LePhy, status: GattStatus) {
        print_info!(
            "Phy updated: addr = {}, tx_phy = {:?}, rx_phy = {:?}, status = {:?}",
            addr,
            tx_phy,
            rx_phy,
            status
        );
    }

    fn on_phy_read(&self, addr: String, tx_phy: LePhy, rx_phy: LePhy, status: GattStatus) {
        print_info!(
            "Phy read: addr = {}, tx_phy = {:?}, rx_phy = {:?}, status = {:?}",
            addr,
            tx_phy,
            rx_phy,
            status
        );
    }

    fn on_search_complete(&self, addr: String, services: Vec<BluetoothGattService>, status: i32) {
        print_info!(
            "GATT DB Search complete: addr = {}, services = {:?}, status = {}",
            addr,
            services,
            status
        );
    }

    fn on_characteristic_read(&self, addr: String, status: i32, handle: i32, value: Vec<u8>) {
        print_info!(
            "GATT Characteristic read: addr = {}, status = {}, handle = {}, value = {:?}",
            addr,
            status,
            handle,
            value
        );
    }

    fn on_characteristic_write(&self, addr: String, status: i32, handle: i32) {
        print_info!(
            "GATT Characteristic write: addr = {}, status = {}, handle = {}",
            addr,
            status,
            handle
        );
    }

    fn on_execute_write(&self, addr: String, status: i32) {
        print_info!("GATT execute write addr = {}, status = {}", addr, status);
    }

    fn on_descriptor_read(&self, addr: String, status: i32, handle: i32, value: Vec<u8>) {
        print_info!(
            "GATT Descriptor read: addr = {}, status = {}, handle = {}, value = {:?}",
            addr,
            status,
            handle,
            value
        );
    }

    fn on_descriptor_write(&self, addr: String, status: i32, handle: i32) {
        print_info!(
            "GATT Descriptor write: addr = {}, status = {}, handle = {}",
            addr,
            status,
            handle
        );
    }

    fn on_notify(&self, addr: String, handle: i32, value: Vec<u8>) {
        print_info!("GATT Notification: addr = {}, handle = {}, value = {:?}", addr, handle, value);
    }

    fn on_read_remote_rssi(&self, addr: String, rssi: i32, status: i32) {
        print_info!("Remote RSSI read: addr = {}, rssi = {}, status = {}", addr, rssi, status);
    }

    fn on_configure_mtu(&self, addr: String, mtu: i32, status: i32) {
        print_info!("MTU configured: addr = {}, mtu = {}, status = {}", addr, mtu, status);
    }

    fn on_connection_updated(
        &self,
        addr: String,
        interval: i32,
        latency: i32,
        timeout: i32,
        status: i32,
    ) {
        print_info!(
            "Connection updated: addr = {}, interval = {}, latency = {}, timeout = {}, status = {}",
            addr,
            interval,
            latency,
            timeout,
            status
        );
    }

    fn on_service_changed(&self, addr: String) {
        print_info!("Service changed for {}", addr,);
    }
}

impl RPCProxy for BtGattCallback {
    fn register_disconnect(&mut self, _f: Box<dyn Fn(u32) + Send>) -> u32 {
        0
    }

    fn get_object_id(&self) -> String {
        self.objpath.clone()
    }

    fn unregister(&mut self, _id: u32) -> bool {
        false
    }
}