FROM scratch
WORKDIR /app
COPY . .
EXPOSE 8811

ENTRYPOINT [ "./tf2_demostats"]
CMD [ "serve" ]
