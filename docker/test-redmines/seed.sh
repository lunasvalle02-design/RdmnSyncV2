#!/usr/bin/env sh
# Siembra los dos Redmine de prueba e imprime sus API keys.
set -e
export MSYS_NO_PATHCONV=1
cd "$(dirname "$0")"
run() { docker compose -f compose.yml exec -T "$@" bundle exec rails runner /seed/seed.rb; }

echo "== Cliente (http://localhost:3001)"
run -e ACTIVITIES="Desarrollar,Pruebas,Reunión,Análisis" \
    -e PROJECT_NAME="Portal Web" -e PROJECT_ID="portal-web" \
    -e ISSUES="Implementar login con OAuth|Corregir error en exportación CSV|Optimizar consultas del dashboard|Revisión de seguridad trimestral" \
    -e ENTRIES="0;0;3;Desarrollar;Flujo de autorización|0;1;2.5;Pruebas;Tests del login|1;1;1.5;Desarrollar;Escapar acentos|2;2;4;Análisis;Perfilado de consultas|3;3;2;Reunión;Reunión con seguridad|-1;4;1;Reunión;Daily semanal" \
    cliente

echo "== Interno (http://localhost:3002)"
run -e ACTIVITIES="Codificar,Testing,Reuniones,Diseño" \
    -e PROJECT_NAME="Proyecto Portal Cliente" -e PROJECT_ID="portal-cliente" \
    -e ISSUES="Implementar el login con OAuth2|Exportación CSV falla con acentos|Seguridad: revisión trimestral|Soporte general" \
    -e ENTRIES="" \
    interno
