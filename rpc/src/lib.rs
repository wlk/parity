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

//! Ethcore rpc.
#![warn(missing_docs)]
#![cfg_attr(feature="nightly", feature(custom_derive, custom_attribute, plugin))]
#![cfg_attr(feature="nightly", plugin(serde_macros, clippy))]

extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
extern crate jsonrpc_core;
extern crate jsonrpc_http_server;
extern crate ethcore_util as util;
extern crate ethcore;
extern crate ethsync;
extern crate transient_hashmap;

use self::jsonrpc_core::{IoHandler, IoDelegate};

pub mod v1;

/// Http server.
pub struct HttpServer {
	handler: IoHandler,
	threads: usize
}

impl HttpServer {
	/// Construct new http server object with given number of threads.
	pub fn new(threads: usize) -> HttpServer {
		HttpServer {
			handler: IoHandler::new(),
			threads: threads
		}
	}

	/// Add io delegate.
	pub fn add_delegate<D>(&mut self, delegate: IoDelegate<D>) where D: Send + Sync + 'static {
		self.handler.add_delegate(delegate);
	}

	/// Start server asynchronously in new thread
	pub fn start_async(self, addr: &str, cors_domain: &str) {
		let server = jsonrpc_http_server::Server::new(self.handler, self.threads);
		server.start_async(addr, jsonrpc_http_server::AccessControlAllowOrigin::Value(cors_domain.to_owned()))
	}
}
