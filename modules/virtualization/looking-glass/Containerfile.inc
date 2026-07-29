RUN --mount=type=bind,from=ctx,source=/modules/virtualization/looking-glass,target=/ctx/modules/virtualization/looking-glass \
    --mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \
    --mount=type=cache,target=/var/cache \
    --mount=type=cache,target=/var/log \
    --mount=type=tmpfs,target=/tmp \
    --mount=type=secret,id=mok_privkey,target=/run/secrets/mok_privkey,required=false \
    FLAVOUR_GATE=desktop FLAVOUR=${FLAVOUR} KERNEL=${KERNEL} bash /ctx/lib/run-module.sh /ctx/modules/virtualization/looking-glass
