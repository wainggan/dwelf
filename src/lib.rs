/**
iterator based image format encoding.
*/

pub mod qoi;

pub trait Format: Default {
	type Header;
	fn decode(self, data: &mut impl std::io::Read) -> Option<(Self::Header, impl Iterator<Item = (u8, u8, u8, u8)>)>;
	fn encode(self, data: impl Iterator<Item = (u8, u8, u8, u8)>, header: Self::Header, out: &mut impl std::io::Write);
}

