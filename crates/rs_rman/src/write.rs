use std::io::Write;

use rs_io::Serialize;

use crate::error::{Error, Result};
use crate::rman::Rman;

impl Serialize for Rman {
    type Error = Error;

    fn to_writer<W: Write>(&self, _writer: &mut W) -> Result<()> {
        Err(Error::Unsupported("rman writing is not implemented"))
    }
}
