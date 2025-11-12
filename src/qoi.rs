
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoiHeaderChannels {
	RGB,
	RGBA,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoiHeaderColorspace {
	Linear,
	SRGB,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QoiHeader {
	pub width: u32,
	pub height: u32,
	pub channels: QoiHeaderChannels,
	pub colorspace: QoiHeaderColorspace,
}

const MAGIC: u32 = u32::from_be_bytes(*b"qoif");

const OP_RGB: u8 = 0b11111110;
const OP_RGBA: u8 = 0b11111111;
const OP_INDEX: u8 = 0b00_000000;
const OP_DIFF: u8 = 0b01_000000;
const OP_LUMA: u8 = 0b10_000000;
const OP_RUN: u8 = 0b11_000000;

const MASK: u8 = 0b11_000000;

#[inline]
fn hash(px: (u8, u8, u8, u8)) -> usize {
	(px.0 as usize) * 3 + (px.1 as usize) * 5 + (px.2 as usize) * 7 + (px.3 as usize) * 11
}

#[derive(Debug, Clone)]
pub struct Qoi;

impl Default for Qoi {
	fn default() -> Self {
		Self
	}
}

impl crate::Format for Qoi {
	type Header = QoiHeader;

	fn decode(self, data: &mut impl std::io::Read) -> Option<(Self::Header, impl Iterator<Item = (u8, u8, u8, u8)>)> {
		
		#[inline]
		fn read<const N: usize>(data: &mut impl std::io::Read) -> Option<[u8; N]> {
			let mut buf = [0; N];
			data.read_exact(&mut buf).ok()?;
			Some(buf)
		}

		// read header

		let magic = u32::from_be_bytes(read(data)?);
		
		if magic != MAGIC {
			return None;
		}

		let width = u32::from_be_bytes(read(data)?);
		let height = u32::from_be_bytes(read(data)?);

		if width == 0 || height == 0 {
			None?;
		}

		let [channels, colorspace] = read(data)?;

		let header = QoiHeader {
			width,
			height,
			channels: match channels {
				3 => QoiHeaderChannels::RGB,
				4 => QoiHeaderChannels::RGBA,
				_ => None?,
			},
			colorspace: match colorspace {
				0 => QoiHeaderColorspace::SRGB,
				1 => QoiHeaderColorspace::Linear,
				_ => None?,
			},
		};

		let mut px = (0, 0, 0, 255);
		let mut array = [(0, 0, 0, 0); 64];

		let mut total = width * height;

		let mut run = 0;

		let iter = core::iter::from_fn(move || {
			if total == 0 {
				None?;
			}

			if run > 0 {
				run -= 1;
				total -= 1;

				return Some(px);
			}

			let [b0] = read(data)?;

			match b0 {
				OP_RGB => {
					let [r, g, b] = read(data)?;
					px.0 = r;
					px.1 = g;
					px.2 = b;

				}
				OP_RGBA => {
					let [r, g, b, a] = read(data)?;
					px.0 = r;
					px.1 = g;
					px.2 = b;
					px.3 = a;

				}
				c if (c & MASK) == OP_INDEX => {
					let index = c & 0b00_111111;
					px = array[index as usize];

				}
				c if (c & MASK) == OP_DIFF => {
					let r_diff = ((c >> 4) & 0b11) as i8 - 2;
					let g_diff = ((c >> 2) & 0b11) as i8 - 2;
					let b_diff = (c & 0b11) as i8 - 2;

					px.0 = px.0.wrapping_add_signed(r_diff);
					px.1 = px.1.wrapping_add_signed(g_diff);
					px.2 = px.2.wrapping_add_signed(b_diff);

				}
				c if (c & MASK) == OP_LUMA => {
					let [b1] = read(data)?;

					let g_diff = (b0 & 0b111111) as i8 - 32;

					let dr_dg = (b1 >> 4) & 0b1111;
					let db_dg = b1 & 0b1111;

					let r_diff = (dr_dg as i8 + g_diff) - 8;
					let b_diff = (db_dg as i8 + g_diff) - 8;

					px.0 = px.0.wrapping_add_signed(r_diff);
					px.1 = px.1.wrapping_add_signed(g_diff);
					px.2 = px.2.wrapping_add_signed(b_diff);

				}
				c if (c & MASK) == OP_RUN => {
					run = c & 0b111111;

				}
				_ => None?,
			}

			array[hash(px) & 63] = px;

			total -= 1;
			Some(px)
		});

		Some((header, iter))
	}

	fn encode(self, data: impl Iterator<Item = (u8, u8, u8, u8)>, header: Self::Header, out: &mut impl std::io::Write) {

		#[inline]
		fn write<const N: usize>(out: &mut impl std::io::Write, input: [u8; N]) {
			_ = out.write(&input);
		}
		
		write(out, MAGIC.to_be_bytes());

		write(out, header.width.to_be_bytes());
		write(out, header.height.to_be_bytes());

		write(
			out,
			[
				match header.channels {
					QoiHeaderChannels::RGB => 3,
					QoiHeaderChannels::RGBA => 4,
				},
				match header.colorspace {
					QoiHeaderColorspace::SRGB => 0,
					QoiHeaderColorspace::Linear => 1,
				},
			],
		);

		let mut px_prev = (0, 0, 0, 255);
		let mut array = [(0, 0, 0, 0); 64];

		let mut run = 0;

		for px in data.take(header.width as usize * header.height as usize) {

			if px == px_prev {
				run += 1;
				if run == 62 {
					write(out, [OP_RUN | (run - 1)]);
					run = 0;
				}

			}
			else {
				if run > 0 {
					write(out, [OP_RUN | (run - 1)]);
					run = 0;
				}

				let index = hash(px) & 63;
				if array[index] == px {
					write(out, [OP_INDEX | index as u8]);

				}
				else if px.3 == px_prev.3 {
					array[index] = px;
					write(out, [OP_RGBA, px.0, px.1, px.2, px.3]);

				}
				else {
					let r_diff = px.0 as i8 - px_prev.0 as i8;
					let g_diff = px.1 as i8 - px_prev.1 as i8;
					let b_diff = px.2 as i8 - px_prev.2 as i8;

					let r_diff_vg = r_diff - g_diff;
					let b_diff_vg = b_diff - g_diff;

					if (-2..=1).contains(&r_diff)
						&& (-2..=1).contains(&g_diff)
						&& (-2..=1).contains(&b_diff)
						{
						let r = ((r_diff + 2) as u8) << 4;
						let g = ((g_diff + 2) as u8) << 2;
						let b = (b_diff + 2) as u8;
						write(out, [OP_DIFF | r | g | b]);

					}
					else if (-8..=7).contains(&r_diff_vg)
						&& (-32..=31).contains(&g_diff)
						&& (-8..=7).contains(&b_diff_vg)
						{
						let r = ((r_diff_vg + 8) as u8) << 4;
						let g = (g_diff + 32) as u8;
						let b = (b_diff_vg + 8) as u8;
						write(out, [OP_LUMA | g, r | b]);

					}
					else {
						write(out, [OP_RGBA, px.0, px.1, px.2]);

					}
				}
			}

			px_prev = px;
		}

		write(out, [0, 0, 0, 0, 0, 0, 0, 1]);
	}
}


#[cfg(test)]
mod test {
    use crate::{Format, qoi};

	const IMAGE_SMALL: &[u8; 44] = include_bytes!("../test/small.qoi");

	#[test]
	fn decode() {
		let mut image = &IMAGE_SMALL[..];

		let (header, iter) = qoi::Qoi.decode(&mut image).expect("error?");

		let data = iter.collect::<Vec<_>>();
		
		assert_eq!(header.width, 4);
		assert_eq!(header.height, 4);
		assert_eq!(header.channels, qoi::QoiHeaderChannels::RGBA);
		assert_eq!(header.colorspace, qoi::QoiHeaderColorspace::SRGB);

		assert_eq!(data.len(), 16);
		assert_eq!(data[0], (0, 0, 0, 255));
		assert_eq!(data[5], (0, 255, 0, 255));
		assert_eq!(data[7], (0, 0, 255, 255));
		assert_eq!(data[13], (255, 0, 0, 255));
	}

	#[test]
	fn encode() {
		let data= &[
			(255, 255, 255, 255),
			(255, 255, 255, 255),
			(0, 255, 255, 255),
			(255, 0, 255, 255),
			(255, 255, 0, 255),
			(255, 255, 255, 255),
		];
		
		let header = qoi::QoiHeader {
			width: 3,
			height: 2,
			channels: qoi::QoiHeaderChannels::RGB,
			colorspace: qoi::QoiHeaderColorspace::Linear,
		};

		let mut out = vec![];

		qoi::Qoi.encode(data.iter().cloned(), header.clone(), &mut out);

		let mut data_write = &out[..];

		let (header_read, iter) = qoi::Qoi.decode(&mut data_write).expect("error?");

		assert_eq!(header.width, header_read.width);
		assert_eq!(header.height, header_read.height);
		assert_eq!(header.channels, header_read.channels);
		assert_eq!(header.colorspace, header_read.colorspace);

		let data_read = iter.collect::<Vec<_>>();

		assert_eq!(&data[..], &data_read);
	}
}

