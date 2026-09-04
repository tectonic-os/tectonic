COPY --from=ctx @MODULE@/hostname /etc/hostname
RUN echo "building ${FLAVOUR}" > /usr/share/early
