# The lineage stage has ended by here, so it can be bound from a stage of its
# own. Whatever a tail fragment opens last is the final image.
FROM rootfs AS greeting
RUN --mount=type=bind,from=rootfs,source=/,target=/rootfs,rw \
    cp /rootfs/etc/os-release /os-release

FROM rootfs
COPY --from=greeting /os-release /usr/share/hello-os-release
