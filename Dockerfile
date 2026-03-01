FROM scratch
ARG TARGETPLATFORM
WORKDIR /app
COPY ${TARGETPLATFORM}/tf2_demostats .
EXPOSE 8811
ENTRYPOINT ["/app/tf2_demostats"]
CMD [ "serve" ]
