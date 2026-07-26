import ritoshark


def test_version_is_exposed():
    assert isinstance(ritoshark.__version__, str)
    assert ritoshark.__version__


def test_exception_hierarchy():
    assert issubclass(ritoshark.ParseError, ritoshark.FormatError)
    assert issubclass(ritoshark.UnsupportedVersion, ritoshark.FormatError)
    assert issubclass(ritoshark.WriteError, ritoshark.FormatError)
    assert issubclass(ritoshark.FormatError, Exception)
