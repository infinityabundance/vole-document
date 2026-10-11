# Phase 21.34.1 scientific-container court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `netcdf_badoffset.nc` | hdf | - | - | true | - | - | 50 | 50 |
| `nc_truncated.nc` | hdf | - | - | true | - | - | 50 | 50 |
| `nc_junk.nc` | hdf | - | - | true | - | - | 50 | 50 |
| `hdf5_filtered.h5` | hdf | - | - | true | - | - | 6 | 6 |
| `hdf5_ohv2.h5` | hdf | - | - | true | - | - | 6 | 6 |
| `hdf5_badmagic.bin` | opaque | - | - | true | - | - | - | 6 |
| `hdf5_truncated.h5` | opaque | - | - | true | - | - | - | 6 |
| `hdf5_junk.bin` | opaque | - | - | true | - | - | - | 6 |
| `prose.txt` | opaque | - | - | true | - | - | - | 6 |
| `netcdf_cdf1.nc` | hdf | 372 | 372 | true | true | true | - | - |
| `netcdf_cdf2.nc` | hdf | 424 | 424 | true | true | true | - | - |
| `hdf5_basic.h5` | hdf | 1209 | 1209 | true | true | true | - | - |
| `hdf5_userblock.h5` | hdf | 1721 | 1721 | true | true | true | - | - |
