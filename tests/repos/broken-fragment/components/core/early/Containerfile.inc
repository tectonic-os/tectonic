COPY --from=ctx @COMPONENT@/hostname /etc/hostname
RUN echo "building ${FLAVOUR}" > /usr/share/early
