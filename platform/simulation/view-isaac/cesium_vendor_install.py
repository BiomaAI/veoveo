from importlib import metadata


def perform_vendor_install():
    """Require the release-bundled wheel installed during the image build."""
    expected_version = "6.0.2"
    try:
        installed_version = metadata.version("lxml")
    except metadata.PackageNotFoundError as error:
        raise RuntimeError(
            "Cesium's bundled lxml wheel was not preinstalled"
        ) from error
    if installed_version != expected_version:
        raise RuntimeError(
            "Cesium requires its bundled lxml "
            f"{expected_version}, found {installed_version}"
        )
