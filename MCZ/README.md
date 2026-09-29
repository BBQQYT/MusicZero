# MCZ — Music Core Zero

MCZ is compiled into the `mz` host. It supplies shared audio metadata, MPRIS on Linux, profile paths, and shutdown handling. It is not a separate runtime executable. Service modules communicate with `mz` through the [module protocol](../README.md#write-a-module) and do not link to MCZ.
