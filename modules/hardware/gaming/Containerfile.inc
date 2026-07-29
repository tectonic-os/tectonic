RUN --mount=type=bind,from=ctx,source=/modules/hardware/gaming,target=/ctx/modules/hardware/gaming \
    --mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \
    --mount=type=cache,target=/var/cache \
    --mount=type=cache,target=/var/log \
    --mount=type=tmpfs,target=/tmp \
    --mount=type=secret,id=mok_privkey,target=/run/secrets/mok_privkey,required=false \
    KERNEL=${KERNEL} bash /ctx/lib/run-module.sh /ctx/modules/hardware/gaming
