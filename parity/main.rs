// Copyright 2015, 2016 Ethcore (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Ethcore client application.

#![warn(missing_docs)]
#![cfg_attr(feature="dev", feature(plugin))]
#![cfg_attr(feature="dev", plugin(clippy))]
extern crate docopt;
extern crate rustc_serialize;
extern crate ethcore_util as util;
extern crate ethcore;
extern crate ethsync;
#[macro_use]
extern crate log as rlog;
extern crate env_logger;
extern crate ctrlc;
extern crate fdlimit;
extern crate daemonize;

#[cfg(feature = "rpc")]
extern crate ethcore_rpc as rpc;

use std::net::{SocketAddr};
use std::env;
use std::process::exit;
use std::path::PathBuf;
use rlog::{LogLevelFilter};
use env_logger::LogBuilder;
use ctrlc::CtrlC;
use util::*;
use util::panics::MayPanic;
use ethcore::spec::*;
use ethcore::client::*;
use ethcore::service::{ClientService, NetSyncMessage};
use ethcore::ethereum;
use ethcore::blockchain::CacheSize;
use ethsync::EthSync;
use docopt::Docopt;
use daemonize::Daemonize;

const USAGE: &'static str = "
Parity. Ethereum Client.
  By Wood/Paronyan/Kotewicz/Drwięga/Volf.
  Copyright 2015, 2016 Ethcore (UK) Limited

Usage:
  parity daemon <pid-file> [options] [ --no-bootstrap | <enode>... ]
  parity [options] [ --no-bootstrap | <enode>... ]

Options:
  --chain CHAIN            Specify the blockchain type. CHAIN may be either a JSON chain specification file
                           or frontier, mainnet, morden, or testnet [default: frontier].
  -d --db-path PATH        Specify the database & configuration directory path [default: $HOME/.parity]
  --keys-path PATH         Specify the path for JSON key files to be found [default: $HOME/.web3/keys]

  --no-bootstrap           Don't bother trying to connect to any nodes initially.
  --listen-address URL     Specify the IP/port on which to listen for peers [default: 0.0.0.0:30304].
  --public-address URL     Specify the IP/port on which peers may connect.
  --address URL            Equivalent to --listen-address URL --public-address URL.
  --peers NUM  	           Try to manintain that many peers [default: 25].
  --no-discovery   	       Disable new peer discovery.
  --upnp                   Use UPnP to try to figure out the correct network settings.
  --node-key KEY           Specify node secret key, either as 64-character hex string or input to SHA3 operation.

  --cache-pref-size BYTES  Specify the prefered size of the blockchain cache in bytes [default: 16384].
  --cache-max-size BYTES   Specify the maximum size of the blockchain cache in bytes [default: 262144].

  -j --jsonrpc             Enable the JSON-RPC API sever.
  --jsonrpc-url URL        Specify URL for JSON-RPC API server [default: 127.0.0.1:8545].

  -l --logging LOGGING     Specify the logging level.
  -v --version             Show information about version.
  -h --help                Show this screen.
";

#[derive(Debug, RustcDecodable)]
struct Args {
	cmd_daemon: bool,
	arg_pid_file: String,
	arg_enode: Vec<String>,
	flag_chain: String,
	flag_db_path: String,
	flag_keys_path: String,
	flag_no_bootstrap: bool,
	flag_listen_address: String,
	flag_public_address: Option<String>,
	flag_address: Option<String>,
	flag_peers: u32,
	flag_no_discovery: bool,
	flag_upnp: bool,
	flag_node_key: Option<String>,
	flag_cache_pref_size: usize,
	flag_cache_max_size: usize,
	flag_jsonrpc: bool,
	flag_jsonrpc_url: String,
	flag_logging: Option<String>,
	flag_version: bool,
}

fn setup_log(init: &Option<String>) {
	let mut builder = LogBuilder::new();
	builder.filter(None, LogLevelFilter::Info);

	if env::var("RUST_LOG").is_ok() {
		builder.parse(&env::var("RUST_LOG").unwrap());
	}

	if let Some(ref s) = *init {
		builder.parse(s);
	}

	builder.init().unwrap();
}

#[cfg(feature = "rpc")]
fn setup_rpc_server(client: Arc<Client>, sync: Arc<EthSync>, url: &str) {
	use rpc::v1::*;

	let mut server = rpc::HttpServer::new(1);
	server.add_delegate(Web3Client::new().to_delegate());
	server.add_delegate(EthClient::new(client.clone(), sync.clone()).to_delegate());
	server.add_delegate(EthFilterClient::new(client).to_delegate());
	server.add_delegate(NetClient::new(sync).to_delegate());
	server.start_async(url);
}

#[cfg(not(feature = "rpc"))]
fn setup_rpc_server(_client: Arc<Client>, _sync: Arc<EthSync>, _url: &str) {
}

fn print_version() {
	println!("\
Parity
  version {}
Copyright 2015, 2016 Ethcore (UK) Limited
License GPLv3+: GNU GPL version 3 or later <http://gnu.org/licenses/gpl.html>.
This is free software: you are free to change and redistribute it.
There is NO WARRANTY, to the extent permitted by law.

By Wood/Paronyan/Kotewicz/Drwięga/Volf.\
", version());
}

fn die_with_message(msg: &str) -> ! {
	println!("ERROR: {}", msg);
	exit(1);
}

#[macro_export]
macro_rules! die {
	($($arg:tt)*) => (die_with_message(&format!("{}", format_args!($($arg)*))));
}

struct Configuration {
	args: Args
}

impl Configuration {
	fn parse() -> Self {
		Configuration {
			args: Docopt::new(USAGE).and_then(|d| d.decode()).unwrap_or_else(|e| e.exit()),
		}
	}

	fn path(&self) -> String {
		self.args.flag_db_path.replace("$HOME", env::home_dir().unwrap().to_str().unwrap())
	}

	fn _keys_path(&self) -> String {
		self.args.flag_keys_path.replace("$HOME", env::home_dir().unwrap().to_str().unwrap())	
	}

	fn spec(&self) -> Spec {
		match self.args.flag_chain.as_ref() {
			"frontier" | "mainnet" => ethereum::new_frontier(),
			"morden" | "testnet" => ethereum::new_morden(),
			"olympic" => ethereum::new_olympic(),
			f => Spec::from_json_utf8(contents(f).unwrap_or_else(|_| die!("{}: Couldn't read chain specification file. Sure it exists?", f)).as_ref()),
		}
	}

	fn normalize_enode(e: &str) -> Option<String> {
		match is_valid_node_url(e) {
			true => Some(e.to_owned()),
			false => None,
		}
	}

	fn init_nodes(&self, spec: &Spec) -> Vec<String> {
		if self.args.flag_no_bootstrap { Vec::new() } else {
			match self.args.arg_enode.len() {
				0 => spec.nodes().clone(),
				_ => self.args.arg_enode.iter().map(|s| Self::normalize_enode(s).unwrap_or_else(||die!("{}: Invalid node address format given for a boot node.", s))).collect(),
			}
		}
	}

	fn net_addresses(&self) -> (Option<SocketAddr>, Option<SocketAddr>) {
		let mut listen_address = None;
		let mut public_address = None;

		if let Some(ref a) = self.args.flag_address {
			public_address = Some(SocketAddr::from_str(a.as_ref()).unwrap_or_else(|_| die!("{}: Invalid listen/public address given with --address", a)));
			listen_address = public_address;
		}
		if listen_address.is_none() {
			listen_address = Some(SocketAddr::from_str(self.args.flag_listen_address.as_ref()).unwrap_or_else(|_| die!("{}: Invalid listen/public address given with --listen-address", self.args.flag_listen_address)));
		}
		if let Some(ref a) = self.args.flag_public_address {
			if public_address.is_some() {
				die!("Conflicting flags provided: --address and --public-address");
			}
			public_address = Some(SocketAddr::from_str(a.as_ref()).unwrap_or_else(|_| die!("{}: Invalid listen/public address given with --public-address", a)));
		}
		(listen_address, public_address)
	}

	fn net_settings(&self, spec: &Spec) -> NetworkConfiguration {
		let mut ret = NetworkConfiguration::new();
		ret.nat_enabled = self.args.flag_upnp;
		ret.boot_nodes = self.init_nodes(spec);
		let (listen, public) = self.net_addresses();
		ret.listen_address = listen;
		ret.public_address = public;
		ret.use_secret = self.args.flag_node_key.as_ref().map(|s| Secret::from_str(&s).unwrap_or_else(|_| s.sha3()));
		ret.discovery_enabled = !self.args.flag_no_discovery;
		ret.ideal_peers = self.args.flag_peers;
		let mut net_path = PathBuf::from(&self.path());
		net_path.push("network");
		ret.config_path = Some(net_path.to_str().unwrap().to_owned());
		ret
	}

	fn execute(&self) {
		if self.args.flag_version {
			print_version();
			return;
		}
		if self.args.cmd_daemon {
			Daemonize::new()
				.pid_file(self.args.arg_pid_file.clone())
				.chown_pid_file(true)
				.start()
				.unwrap_or_else(|e| die!("Couldn't daemonize; {}", e));
		}
		self.execute_client();
	}

	fn execute_client(&self) {
		// Setup logging
		setup_log(&self.args.flag_logging);
		// Raise fdlimit
		unsafe { ::fdlimit::raise_fd_limit(); }

		let spec = self.spec();
		let net_settings = self.net_settings(&spec);

		// Build client
		let mut service = ClientService::start(spec, net_settings, &Path::new(&self.path())).unwrap();
		let client = service.client().clone();
		client.configure_cache(self.args.flag_cache_pref_size, self.args.flag_cache_max_size);

		// Sync
		let sync = EthSync::register(service.network(), client);

		// Setup rpc
		if self.args.flag_jsonrpc {
			SocketAddr::from_str(&self.args.flag_jsonrpc_url).unwrap_or_else(|_|die!("{}: Invalid JSONRPC listen address given with --jsonrpc-url. Should be of the form 'IP:port'.", self.args.flag_jsonrpc_url));
			setup_rpc_server(service.client(), sync.clone(), &self.args.flag_jsonrpc_url);
		}

		// Register IO handler
		let io_handler  = Arc::new(ClientIoHandler {
			client: service.client(),
			info: Default::default(),
			sync: sync
		});
		service.io().register_handler(io_handler).expect("Error registering IO handler");

		// Handle exit
		wait_for_exit(&service);
	}
}

fn wait_for_exit(client_service: &ClientService) {
	let exit = Arc::new(Condvar::new());

	// Handle possible exits
	let e = exit.clone();
	CtrlC::set_handler(move || { e.notify_all(); });
	let e = exit.clone();
	client_service.on_panic(move |_reason| { e.notify_all(); });

	// Wait for signal
	let mutex = Mutex::new(());
	let _ = exit.wait(mutex.lock().unwrap()).unwrap();
}

fn main() {
	Configuration::parse().execute();
}

struct Informant {
	chain_info: RwLock<Option<BlockChainInfo>>,
	cache_info: RwLock<Option<CacheSize>>,
	report: RwLock<Option<ClientReport>>,
}

impl Default for Informant {
	fn default() -> Self {
		Informant {
			chain_info: RwLock::new(None),
			cache_info: RwLock::new(None),
			report: RwLock::new(None),
		}
	}
}

impl Informant {
	pub fn tick(&self, client: &Client, sync: &EthSync) {
		// 5 seconds betwen calls. TODO: calculate this properly.
		let dur = 5usize;

		let chain_info = client.chain_info();
		let queue_info = client.queue_info();
		let cache_info = client.cache_info();
		let report = client.report();
		let sync_info = sync.status();

		if let (_, &Some(ref last_cache_info), &Some(ref last_report)) = (self.chain_info.read().unwrap().deref(), self.cache_info.read().unwrap().deref(), self.report.read().unwrap().deref()) {
			println!("[ #{} {} ]---[ {} blk/s | {} tx/s | {} gas/s  //··· {}/{} peers, #{}, {}+{} queued ···//  {} ({}) bl  {} ({}) ex ]",
				chain_info.best_block_number,
				chain_info.best_block_hash,
				(report.blocks_imported - last_report.blocks_imported) / dur,
				(report.transactions_applied - last_report.transactions_applied) / dur,
				(report.gas_processed - last_report.gas_processed) / From::from(dur),

				sync_info.num_active_peers,
				sync_info.num_peers,
				sync_info.last_imported_block_number.unwrap_or(chain_info.best_block_number),
				queue_info.unverified_queue_size,
				queue_info.verified_queue_size,

				cache_info.blocks,
				cache_info.blocks as isize - last_cache_info.blocks as isize,
				cache_info.block_details,
				cache_info.block_details as isize - last_cache_info.block_details as isize
			);
		}

		*self.chain_info.write().unwrap().deref_mut() = Some(chain_info);
		*self.cache_info.write().unwrap().deref_mut() = Some(cache_info);
		*self.report.write().unwrap().deref_mut() = Some(report);
	}
}

const INFO_TIMER: TimerToken = 0;

struct ClientIoHandler {
	client: Arc<Client>,
	sync: Arc<EthSync>,
	info: Informant,
}

impl IoHandler<NetSyncMessage> for ClientIoHandler {
	fn initialize(&self, io: &IoContext<NetSyncMessage>) {
		io.register_timer(INFO_TIMER, 5000).expect("Error registering timer");
	}

	fn timeout(&self, _io: &IoContext<NetSyncMessage>, timer: TimerToken) {
		if INFO_TIMER == timer {
			self.info.tick(&self.client, &self.sync);
		}
	}
}

/// Parity needs at least 1 test to generate coverage reports correctly.
#[test]
fn if_works() {
}
