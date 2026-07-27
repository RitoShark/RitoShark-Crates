__version__: str

class FormatError(Exception): ...
class ParseError(FormatError): ...
class UnsupportedVersion(FormatError): ...
class WriteError(FormatError): ...

class Submesh:
    name: str
    vertex_start: int
    vertex_count: int
    index_start: int
    index_count: int
    def __init__(
        self,
        name: str,
        vertex_start: int,
        vertex_count: int,
        index_start: int,
        index_count: int,
    ) -> None: ...

class Skn:
    @staticmethod
    def from_path(path: str) -> Skn: ...
    @staticmethod
    def from_bytes(data: bytes) -> Skn: ...
    @staticmethod
    def new(
        *,
        positions: bytes,
        normals: bytes,
        uvs: bytes,
        blend_indices: bytes,
        blend_weights: bytes,
        indices: bytes,
        submeshes: list[Submesh],
    ) -> Skn:
        """Builds a new .skn from packed buffers. `indices` values above 65535 or
        `blend_indices` values above 255 raise WriteError."""
    @property
    def version(self) -> tuple[int, int]: ...
    @property
    def vertex_count(self) -> int: ...
    @property
    def positions(self) -> bytes:
        """Tightly packed little-endian, 3 x float32 per vertex."""
    @property
    def normals(self) -> bytes:
        """Tightly packed little-endian, 3 x float32 per vertex."""
    @property
    def uvs(self) -> bytes:
        """Tightly packed little-endian, 2 x float32 per vertex."""
    @property
    def blend_indices(self) -> bytes:
        """Tightly packed little-endian, 4 x uint32 per vertex."""
    @property
    def blend_weights(self) -> bytes:
        """Tightly packed little-endian, 4 x float32 per vertex."""
    @property
    def indices(self) -> bytes:
        """Tightly packed little-endian, 1 x uint32 per corner. Narrowed to uint16 on
        write; values above 65535 raise WriteError."""
    @property
    def submeshes(self) -> list[Submesh]: ...
    def to_bytes(self) -> bytes: ...
    def to_path(self, path: str) -> None: ...

class ScbFace:
    material: str
    indices: tuple[int, int, int]
    uvs: tuple[tuple[float, float], tuple[float, float], tuple[float, float]]
    def __init__(
        self,
        material: str,
        indices: tuple[int, int, int],
        uvs: tuple[tuple[float, float], tuple[float, float], tuple[float, float]],
    ) -> None: ...

class Scb:
    @staticmethod
    def from_path(path: str) -> Scb: ...
    @staticmethod
    def from_bytes(data: bytes) -> Scb: ...
    @staticmethod
    def new(name: str, positions: bytes, faces: list[ScbFace]) -> Scb: ...
    @property
    def name(self) -> str: ...
    @property
    def central(self) -> tuple[float, float, float]: ...
    @property
    def positions(self) -> bytes:
        """Tightly packed little-endian, 3 x float32 per vertex."""
    @property
    def faces(self) -> list[ScbFace]: ...
    def to_bytes(self) -> bytes: ...
    def to_path(self, path: str) -> None: ...

class Sco:
    """Read-only: the game dropped the text .sco format, so rs_mesh writes only the
    binary .scb form. There is no to_bytes/to_path."""

    @staticmethod
    def from_path(path: str) -> Sco: ...
    @staticmethod
    def from_bytes(data: bytes) -> Sco: ...
    @property
    def name(self) -> str: ...
    @property
    def central(self) -> tuple[float, float, float]: ...
    @property
    def positions(self) -> bytes:
        """Tightly packed little-endian, 3 x float32 per vertex."""
    @property
    def faces(self) -> list[ScbFace]: ...

class Joint:
    name: str
    id: int
    parent_id: int
    radius: float
    hash: int
    flags: int
    local_translation: tuple[float, float, float]
    local_scale: tuple[float, float, float]
    local_rotation: tuple[float, float, float, float]
    inverse_bind_translation: tuple[float, float, float]
    inverse_bind_scale: tuple[float, float, float]
    inverse_bind_rotation: tuple[float, float, float, float]
    def __init__(
        self,
        name: str,
        id: int,
        parent_id: int,
        radius: float,
        local_translation: tuple[float, float, float],
        local_scale: tuple[float, float, float],
        local_rotation: tuple[float, float, float, float],
        inverse_bind_translation: tuple[float, float, float],
        inverse_bind_scale: tuple[float, float, float],
        inverse_bind_rotation: tuple[float, float, float, float],
        flags: int = 0,
    ) -> None: ...

class Skl:
    @staticmethod
    def from_path(path: str) -> Skl: ...
    @staticmethod
    def from_bytes(data: bytes) -> Skl: ...
    @staticmethod
    def new(
        joints: list[Joint],
        influences: list[int],
        name: str = "",
        asset: str = "",
    ) -> Skl: ...
    @property
    def name(self) -> str: ...
    @property
    def asset(self) -> str: ...
    @property
    def joints(self) -> list[Joint]: ...
    @property
    def influences(self) -> list[int]: ...
    def to_bytes(self) -> bytes: ...
    def to_path(self, path: str) -> None: ...

class AnimFrame:
    time: float
    rotation: tuple[float, float, float, float]
    translation: tuple[float, float, float]
    scale: tuple[float, float, float]
    def __init__(
        self,
        time: float,
        rotation: tuple[float, float, float, float],
        translation: tuple[float, float, float],
        scale: tuple[float, float, float],
    ) -> None: ...

class AnimTrack:
    joint_hash: int
    frames: list[AnimFrame]
    def __init__(self, joint_hash: int, frames: list[AnimFrame]) -> None: ...

class Anm:
    @staticmethod
    def from_path(path: str) -> Anm: ...
    @staticmethod
    def from_bytes(data: bytes) -> Anm: ...
    @staticmethod
    def new(fps: float, tracks: list[AnimTrack]) -> Anm: ...
    @property
    def fps(self) -> float: ...
    @property
    def frame_count(self) -> int: ...
    @property
    def is_byte_exact(self) -> bool:
        """True when the original source bytes are still reproduced exactly on write.
        Becomes False once make_editable() is called."""
    def make_editable(self) -> None:
        """Switches from byte-exact passthrough to an editable track representation.
        After this call, to_bytes()/to_path() emit tracks rather than the original bytes."""
    @property
    def tracks(self) -> list[AnimTrack]: ...
    def to_bytes(self) -> bytes: ...
    def to_path(self, path: str) -> None: ...

class Tex:
    """Textures have no writer: .tex encoding is out of scope for the Python bindings."""

    @staticmethod
    def from_path(path: str) -> Tex: ...
    @staticmethod
    def from_bytes(data: bytes) -> Tex: ...
    @property
    def width(self) -> int: ...
    @property
    def height(self) -> int: ...
    @property
    def format(self) -> str: ...
    @property
    def mip_count(self) -> int: ...
    @property
    def rgba(self) -> bytes:
        """Decodes on every access. 4 x uint8 per pixel, row-major, top-down.
        Empty if width or height is 0."""
    @property
    def rgba_f32(self) -> bytes:
        """Decodes on every access. 4 x float32 per pixel in 0..1, row-flipped
        bottom-up to match Blender's image.pixels layout. Empty if width or
        height is 0."""

class WadChunk:
    path_hash: int
    compressed_size: int
    uncompressed_size: int
    compression: str
    is_duplicated: bool

class Wad:
    """Read-only: building .wad archives is out of scope for the Python bindings."""

    @staticmethod
    def from_path(path: str) -> Wad: ...
    @staticmethod
    def from_bytes(data: bytes) -> Wad: ...
    @property
    def version(self) -> tuple[int, int]: ...
    @property
    def chunks(self) -> list[WadChunk]: ...
    def read(self, path_hash: int) -> bytes | None:
        """Decompresses and returns the chunk with the given path hash, or None if no
        such chunk exists."""
    def read_path(self, path: str) -> bytes | None:
        """Equivalent to read(wad_hash(path))."""
    def __len__(self) -> int: ...

def wad_hash(path: str) -> int:
    """xxh64 of the lowercased path, seed 0 — the WAD chunk path hash. Always lowercases
    internally; never lowercase the input yourself before calling this."""

def read_bin(path: str) -> dict:
    """Reads a .bin file into a plain Python dict. This view is lossy by design: it drops
    the hash-versus-resolved-name duality, the LIST/LIST2 distinction, and duplicate map
    keys. There is no corresponding write function."""

def read_bin_bytes(data: bytes) -> dict:
    """Same as read_bin but from an in-memory buffer."""

class MapSubmesh:
    hash: int
    name: str
    index_start: int
    index_count: int
    min_vertex: int
    max_vertex: int

class MapModel:
    """Does not own its vertex/index data: positions() and indices() de-interleave the
    referenced shared buffer on every call, so they are methods, not properties. There is
    no writer; only read and byte-exact re-emission of the containing MapGeo are exposed."""

    @property
    def name(self) -> str: ...
    @property
    def vertex_count(self) -> int: ...
    @property
    def layer(self) -> int: ...
    @property
    def transform(self) -> list[float]:
        """16 floats, column-major 4x4 matrix."""
    @property
    def bounds(self) -> tuple[tuple[float, float, float], tuple[float, float, float]]: ...
    @property
    def disable_backface_culling(self) -> bool: ...
    @property
    def texture_overrides(self) -> list[tuple[int, str]]: ...
    @property
    def submeshes(self) -> list[MapSubmesh]: ...
    def positions(self) -> bytes:
        """METHOD, not a property. Tightly packed little-endian, 3 x float32 per vertex."""
    def indices(self) -> bytes:
        """METHOD, not a property. Tightly packed little-endian, 1 x uint32 per corner."""

class MapGeo:
    """Construction from scratch is out of scope: the format carries scene graphs,
    bucketed geometry, planar reflectors, and per-version lighting that a DCC does not
    author. Only read and byte-exact re-emission are exposed."""

    @staticmethod
    def from_path(path: str) -> MapGeo: ...
    @staticmethod
    def from_bytes(data: bytes) -> MapGeo: ...
    @property
    def version(self) -> int: ...
    @property
    def models(self) -> list[MapModel]: ...
    def to_bytes(self) -> bytes: ...
    def to_path(self, path: str) -> None: ...
    def __len__(self) -> int: ...
