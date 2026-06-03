/*!
rs_mapgeo reads and writes League `.mapgeo` (OEGM) environment geometry. It supports on-disk
versions 14, 17 and 18, parsing the full structure: shader/texture overrides, vertex declarations,
the raw vertex and index buffers, the list of placed models (buffer references, submeshes,
transform, bounding box, visibility layer, render flags and per-version lighting), and the trailing
bucketed scene graphs and planar reflectors. The writer is the byte-exact inverse for every version
it reads. Any other on-disk version is reported as `Error::UnsupportedVersion`.
*/

#![forbid(unsafe_code)]

mod error;
mod mapgeo;
mod read;
mod write;

pub use error::{Error, Result};
pub use mapgeo::{
    AssetChannel, ElementFormat, ElementName, GeometryBucket, IndexBuffer, MapGeometry, MapModel,
    PlanarReflector, SceneGraph, Submesh, TextureOverride, VertexBuffer, VertexDescription,
    VertexElement, VertexUsage,
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
