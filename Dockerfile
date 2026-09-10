# Keep in sync with the release runner (ubuntu-latest): the binary is built
# for gnu Linux and dynamically links the runner's glibc.
FROM ubuntu:24.04
ARG TARGETPLATFORM
WORKDIR /app
COPY ${TARGETPLATFORM}/tf2_demostats .
EXPOSE 8811
ENTRYPOINT ["/app/tf2_demostats"]
CMD [ "serve" ]
