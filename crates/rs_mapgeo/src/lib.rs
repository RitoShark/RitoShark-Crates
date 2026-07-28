/*!
rs_mapgeo reads and writes League `.mapgeo` (OEGM) environment geometry. It supports on-disk
versions 5, 6, 7, 9, 11, 12, 13, 14, 15, 17 and 18, parsing the full structure: shader/texture
overrides, vertex declarations, the raw vertex and index buffers, the list of placed models (buffer
references, submeshes, transform, bounding box, visibility layer, render flags and per-version
lighting), and the trailing bucketed scene graphs and planar reflectors. The writer is the
byte-exact inverse for every version it reads. Versions 8, 10 and 16 are not defined by the
reference oracle and are reported as `Error::UnsupportedVersion`.

On top of that it supports editing a parsed file: replacing a model's geometry, appending a model
and removing one, with the vertex codec needed to write attributes into whichever interleaved layout
the target description declares. Editing is expressed against a file that was read rather than as
construction from nothing, because the scene graphs, baked lighting and shader tables a `.mapgeo`
carries are not derivable from geometry.
*/

#![forbid(unsafe_code)]

mod edit;
mod error;
mod mapgeo;
mod read;
mod vertex;
mod write;

pub use edit::{Geometry, SubmeshRange};
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
