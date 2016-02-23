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

//! Diff misc.

use std::fs::File;
use common::*;
use target_info::Target;
use rustc_version;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

#[derive(Debug,Clone,PartialEq,Eq)]
/// Diff type for specifying a change (or not).
pub enum Diff<T> where T: Eq {
	/// Both sides are the same.
	Same,
	/// Left (pre, source) side doesn't include value, right side (post, destination) does.
	Born(T),
	/// Both sides include data; it chaged value between them.
	Changed(T, T),
	/// Left (pre, source) side does include value, right side (post, destination) does not.
	Died(T),
}

impl<T> Diff<T> where T: Eq {
	/// Construct new object with given `pre` and `post`.
	pub fn new(pre: T, post: T) -> Self { if pre == post { Diff::Same } else { Diff::Changed(pre, post) } }

	/// Get the before value, if there is one.
	pub fn pre(&self) -> Option<&T> { match *self { Diff::Died(ref x) | Diff::Changed(ref x, _) => Some(x), _ => None } }

	/// Get the after value, if there is one.
	pub fn post(&self) -> Option<&T> { match *self { Diff::Born(ref x) | Diff::Changed(_, ref x) => Some(x), _ => None } }

	/// Determine whether there was a change or not.
	pub fn is_same(&self) -> bool { match *self { Diff::Same => true, _ => false }}
}

#[derive(PartialEq,Eq,Clone,Copy)]
/// Boolean type for clean/dirty status.
pub enum Filth {
	/// Data has not been changed.
	Clean,
	/// Data has been changed.
	Dirty,
}

/// Read the whole contents of a file `name`.
pub fn contents(name: &str) -> Result<Bytes, UtilError> {
	let mut file = try!(File::open(name));
	let mut ret: Vec<u8> = Vec::new();
	try!(file.read_to_end(&mut ret));
	Ok(ret)
}

/// Get the standard version string for this software.
pub fn version() -> String {
	format!("Parity//{}-{}-{}/{}-{}-{}/rustc{}", env!("CARGO_PKG_VERSION"), short_sha(), commit_date().replace("-", ""), Target::arch(), Target::os(), Target::env(), rustc_version::version())
}