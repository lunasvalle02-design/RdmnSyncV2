# RdmnSync

Aplicación de escritorio (Rust + egui) para trasladar tus imputaciones de horas de un Redmine a otro,
con emparejamiento automático de issues y mapeo aprendido de actividades.

## Funcionalidades

- **Conexiones**: al arrancar se piden las API keys de cada Redmine. Opcionalmente se recuerdan en el
  almacén de credenciales del sistema (Administrador de credenciales de Windows / Secret Service en Linux).
- **Explorador**: tus proyectos, los issues asignados (con filtros), su detalle y sus imputaciones.
- **Sincronizar** (origen → destino, en un solo sentido):
  - Lee tus imputaciones del origen en un periodo y busca el issue equivalente en el destino:
    - referencia explícita (la URL de un issue en la descripción del otro) → 100 %;
    - similitud del asunto (tolerante a acentos, plurales, `OAuth`/`OAuth2`…), proyecto mapeado y asignación.
  - Por encima del umbral automático (92 % por defecto) se enlaza solo; entre umbrales **pregunta**
    mostrando candidatos; por debajo propone **crear el issue** en el destino copiando tipo, prioridad,
    fechas, estimación, % realizado y campos personalizados con el mismo nombre.
  - **Actividades**: si el nombre es idéntico se mapea solo; si no («Desarrollar» → «Codificar»)
    sugiere y **siempre pide confirmación**. Se recuerda por pareja de Redmines.
  - Evita duplicados con tres mecanismos: marca `[sync:<origen>#<id>]` en el comentario (solo en el
    destino), historial local, y detección de imputaciones iguales (día + horas + issue).
- **Proyectos**: se mapean al confirmar imputaciones sin issue, al crear un issue, a mano en «Mapeos»
  o confirmando la sugerencia que hace la app cuando la mayoría de issues enlazados de un proyecto de
  origen están en el mismo proyecto de destino.
- **Mapeos**: revisar/editar actividades, proyectos e issues aprendidos; **reglas por comentario**
  (p. ej. «reunión» → «Reuniones»); **historial con deshacer** (borra del destino lo creado en un envío).
- **Resumen semanal**: horas por día en cada Redmine y diferencias, con acceso directo a sincronizar.
- Tema oscuro/claro, tipografía Inter e iconos Phosphor.

## Compilar (Docker, sin toolchains locales)

```sh
sh scripts/build.sh          # o, en PowerShell: scripts\build.ps1
```

Genera `dist/rdmnsync-windows-x64.exe` y `dist/rdmnsync-linux-x64`. Para comandos sueltos de cargo:
`scripts/cargo.sh check --target x86_64-pc-windows-gnu`.

## Entorno de pruebas

`docker/test-redmines` levanta dos Redmine 6 con actividades distintas y datos de ejemplo:

```sh
docker compose -f docker/test-redmines/compose.yml up -d
sh docker/test-redmines/seed.sh        # imprime las API keys
DOCKER_EXTRA="-e RDMN_A_URL=http://host.docker.internal:3001 -e RDMN_A_KEY=<clave> \
              -e RDMN_B_URL=http://host.docker.internal:3002 -e RDMN_B_KEY=<clave>" \
  scripts/cargo.sh test --target x86_64-unknown-linux-gnu -- --include-ignored --nocapture
```

El test `end_to_end` analiza, envía y vuelve a analizar comprobando que no se duplica nada, y borra
lo que crea. En compilaciones *debug*, `RDMNSYNC_DEV_TAB=sync|explorer|…` salta el login y abre una
pestaña (útil para capturas).

## Estructura

| Módulo | Responsabilidad |
|---|---|
| `redmine/` | Cliente REST (ureq, TLS con certificados del sistema) y modelos |
| `matching.rs` | Normalización, similitud, familias de actividades, marca `[sync:…]` |
| `sync.rs` | Carga de datos, construcción del plan, deduplicación y ejecución |
| `store.rs` | Configuración, mapeos e historial en JSON (`%APPDATA%\RdmnSync` / `~/.config/rdmnsync`) |
| `secrets.rs` | API keys en el almacén del sistema |
| `ui/` | Pantallas egui; `theme.rs` contiene la paleta y componentes |

## Limitaciones conocidas

- Solo migra imputaciones del propio usuario (el de la API key).
- Si el destino exige campos personalizados obligatorios que ningún issue del proyecto tiene
  rellenos, hay que completarlos en el diálogo (el error de Redmine se muestra tal cual).
- Las actividades se resuelven por nombre dentro del proyecto destino: si un proyecto tiene
  deshabilitada la actividad mapeada, esa imputación falla con un mensaje claro.
