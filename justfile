fix:
    cargo fix --lib -p tf2_demostats --allow-dirty

test_post:
    curl -v -i --form "file=@test.dem" http://localhost:8811/
