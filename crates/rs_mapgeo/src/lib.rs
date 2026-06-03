/*!
rs_mapgeo reads and writes League `.mapgeo` (OEGM) environment geometry. It targets version 17,
the current shipping format, and parses the full top-level structure: shader/texture overrides,
vertex declarations, the raw vertex and index buffers, and the list of placed models with their
buffer references, submeshes, transform, bounding box, visibility layer, and render flags. The
writer is the byte-exact inverse of that top-level layout. The trailing bucketed scene graph and
planar reflectors are out of scope for this MVP — reading stops cleanly after the model list — and
any other on-disk version is reported as `Error::UnsupportedVersion`.
*/

#![forbid(unsafe_code)]

mod error;
mod mapgeo;
mod read;
mod write;

pub use error::{Error, Result};
pub use mapgeo::{
    AssetChannel, ElementFormat, ElementName, IndexBuffer, MapGeometry, MapModel, Submesh,
    TextureOverride, VertexBuffer, VertexDescription, VertexElement, VertexUsage,
};

impl rs_io::Parse for MapGeometry {
    type Error = Error;

    fn from_reader<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<Self> {
        MapGeometry::from_reader(reader)
    }
}

impl rs_io::Serialize for MapGeometry {
    type Error = Error;

    fn to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        MapGeometry::to_writer(self, writer)
    }
}
