# CLAUDE.md

Guía para Claude Code (y para cualquier persona) que trabaje en este repositorio.

## Qué es

RdmnSync: aplicación de escritorio en **Rust + egui/eframe 0.37** que traslada las imputaciones
(*time entries*) del usuario de un Redmine **origen** a uno **destino**, emparejando issues, mapeando
actividades, estados y versiones, e igualando opcionalmente los issues de destino.

- Documentación de usuario: `README.md` (visión general) y `MANUAL.md` (uso pantalla a pantalla).
- Mejoras pendientes: `PROPUESTAS.md` (propuestas `P-01`…`P-46` con valor, esfuerzo y versión objetivo).

## Reglas que no se rompen

1. **El Redmine de origen es de solo lectura.** Ningún código puede escribir (POST, PUT o DELETE)
   contra el cliente de origen. Toda escritura va al destino. El test `end_to_end` lo verifica.
2. **Las API keys nunca se escriben en disco ni en logs.** Viven en `secrets.rs` (almacén del
   sistema) y en memoria. `store.rs` no debe contener claves.
3. **Compatibilidad del fichero de configuración.** Todo campo nuevo en `Store`, `Settings`,
   `PairMappings`, `IssueLink`… debe llevar `#[serde(default)]` (o el struct entero), para que las
   configuraciones existentes sigan cargando. Si no se puede leer, `Store::load` hace `.json.bak`.
4. **Nada de duplicados.** Cualquier cambio en el envío debe mantener los tres mecanismos: marca
   `[sync:<instancia>#<id>]` (solo en el destino), historial (`ledger`) y detección por día, horas e
   issue.
5. **Preguntar ante la duda.** La app empareja sola solo con alta confianza; los nombres distintos
   (actividades, estados) siempre se confirman. No introducir automatismos que decidan sin el usuario
   en casos dudosos.

## Compilar y probar (siempre en Docker)

No hay toolchain de Rust en local: todo pasa por la imagen `rdmnsync-builder` (`docker/Dockerfile`).

```sh
scripts/cargo.sh check --target x86_64-unknown-linux-gnu --message-format short
scripts/cargo.sh test  --target x86_64-unknown-linux-gnu
scripts/cargo.sh build --release --target x86_64-pc-windows-gnu
sh scripts/build.sh            # tests + binarios release de Windows y Linux en dist/
```

- En Git Bash de Windows, los comandos `docker` con rutas necesitan `MSYS_NO_PATHCONV=1` (los scripts
  ya lo gestionan).
- `RDMNSYNC_TARGET_VOLUME` elige el volumen de compilación (por defecto `rdmnsync-target`). El registro
  de crates (`rdmnsync-cargo-registry`) se comparte sin problema.
- Si el usuario tiene abierto `dist/rdmnsync-windows-x64.exe`, Windows bloquea el fichero y la copia
  final de `build.sh` falla. **No cierres su proceso**: copia los binarios a otra carpeta.

### Entorno de pruebas

```sh
docker compose -f docker/test-redmines/compose.yml up -d     # :3001 «cliente», :3002 «interno»
sh docker/test-redmines/seed.sh                              # idempotente; imprime las API keys
DOCKER_EXTRA="-e RDMN_A_URL=http://host.docker.internal:3001 -e RDMN_A_KEY=<A> \
              -e RDMN_B_URL=http://host.docker.internal:3002 -e RDMN_B_KEY=<B>" \
  scripts/cargo.sh test --target x86_64-unknown-linux-gnu -- --include-ignored --nocapture
```

El test de extremo a extremo crea y borra datos reales en esos Redmines: **ejecútalo de uno en uno**,
nunca dos a la vez. Para ampliar los casos, añade variables en `seed.sh` y `seed.rb`.

### Ver la interfaz

En compilaciones debug, `RDMNSYNC_DEV_TAB=explorer|sync|mappings|summary|settings` salta el login
(si hay dos conexiones) y abre la pestaña. Con `sync`, además analiza; `RDMNSYNC_DEV_CREATE=1` abre
el diálogo de creación de issue. Para capturar la ventana en Windows usa `PrintWindow` (renderiza
solo la ventana de la app). **No captures la pantalla completa**, porque puede mostrar contenido
privado del usuario. Los clics sintéticos (`PostMessage`) no llegan a winit sin foco.

Para conectar la app a los Redmines de prueba sin interacción: escribe
`%APPDATA%\RdmnSync\RdmnSync\config\rdmnsync.json` y guarda las claves con
`cmdkey /generic:<id>.RdmnSync /user:<id> /pass:<clave>`. **Al terminar, borra ambas cosas** y para
los contenedores; no dejes configuración de prueba en el equipo del usuario.

## Arquitectura

| Módulo | Responsabilidad |
|---|---|
| `src/redmine/` | Cliente REST bloqueante (ureq 3; `http_status_as_error(false)` para leer errores 422) y modelos serde |
| `src/matching.rs` | `normalize`, `similarity`, familias de sinónimos (`NAME_FAMILIES`), ranking de candidatos, marca `[sync:…]` |
| `src/sync.rs` | `load` (datos de ambos lados), `Plan` (issues, proyectos, actividades, filas), duplicados, `execute` |
| `src/issue_sync.rs` | `diff` campo a campo para igualar issues de destino, `apply` + relectura para detectar campos ignorados |
| `src/store.rs` | Configuración JSON: instancias, `PairMappings` por pareja dirigida `"src->dst"`, `ledger`, `Settings` |
| `src/secrets.rs` | `keyring` (Windows Credential Manager / Secret Service) |
| `src/worker.rs` | Hilos de fondo; los resultados vuelven como `app::Msg` y se procesan al inicio de cada frame |
| `src/app.rs` | `App`, `Session`, `Msg`, barra superior, avisos, utilidades de fechas, piloto automático debug |
| `src/theme.rs` | Paletas claro/oscuro, fuentes, componentes (`card`, `badge`, `pill`, `primary_button`, `stat`…) |
| `src/ui/*.rs` | Una pantalla por fichero; cada una expone `show(app, ui)` y sus `on_*` para mensajes |

Flujo de una operación de red: la UI llama a `app.worker.spawn(move || Msg::X { … })` con un `Client`
clonado y procesa `Msg::X` en `App::handle`. Usa contadores de generación (`seq`) para descartar
respuestas obsoletas. Nunca bloquees el hilo de la interfaz.

## Convenciones

- **Idioma**: interfaz, comentarios, mensajes de commit y documentación en **español**. Identificadores
  de código en inglés.
- **Estilo**: el del código existente (comentarios `//!` de módulo y `///` breves; sin comentarios
  obvios). Sin warnings: `cargo check` debe salir limpio.
- **UI**: usa los componentes de `theme.rs` y la paleta (`theme::palette(ui)`); iconos de
  `egui_phosphor::regular` vía `theme::icon`. Textos con plural correcto (`plural()` en `sync_view.rs`).
- **Borrow checker en egui**: dentro de closures de `theme::card`/`Grid`, no llames a métodos `&self`
  de `App` mientras tengas prestado un campo. Precalcula los datos fuera o recoge acciones en vectores
  y aplícalas después del closure (patrón usado en todas las pantallas).
- **Edición 2024**: `gen` es palabra reservada; se admiten `let`-chains (`if let … && let …`).

### Particularidades de egui 0.37 (cambian respecto a versiones conocidas)

- `eframe::App::ui(&mut self, ui, frame)`; paneles con `egui::Panel::left/right/top(id).show(ui, …)`
  y `CentralPanel::default().show(ui, …)`.
- `egui::Id::new` está obsoleto: usa `Id::unique` o una sal. `ComboBox::from_id_salt` admite tuplas.
- `Frame::new()`, `CornerRadius`, `Margin` con enteros (`i8`), `Shadow` con `[i8; 2]`/`u8`.
- `egui-phosphor 0.14` depende de egui 0.36: solo se usan sus constantes y `font_bytes()`, nunca
  `add_to_fonts`. Inter se distribuye **sin el rango Unicode privado** (U+E000–F8FF) para no tapar los
  iconos; si actualizas la fuente, vuelve a recortarla con `pyftsubset`.
- Las *features* de eframe se eligen a mano (`default-features = false`); `clipboard` y `links` son
  imprescindibles (Ctrl+V y abrir enlaces).

## Git

- Rama principal `main`, remoto `origin` (GitHub `lunasvalle02-design/RdmnSyncV2`).
- `.gitattributes` fuerza LF (los `.sh` se ejecutan en Linux); `.ps1` en CRLF.
- No hagas commit ni push sin que el usuario lo pida. Antes de subir, revisa que no haya claves ni
  binarios (`dist/` y `target/` están ignorados).
- Mensajes de commit en español: título breve y lista de cambios.

## Trabajar en PROPUESTAS.md con agentes en worktrees

Las propuestas de `PROPUESTAS.md` están pensadas para resolverse **una por agente**, cada una en su
propio worktree y rama, de modo que puedan avanzar en paralelo sin interferir.

### Cuándo usarlo

Solo cuando el usuario lo pida (p. ej. «implementa P-14 y P-28 con agentes»). Para una propuesta
pequeña sin paralelismo, trabaja directamente en una rama.

### Convenciones

- **Rama**: `propuesta/P-XX-descripcion-corta` (p. ej. `propuesta/P-14-deshacer-igualado`).
- **Lanzamiento**: herramienta `Agent` con `isolation: "worktree"`, una propuesta por agente.
- **Volumen de compilación propio**: cada agente usa `RDMNSYNC_TARGET_VOLUME=rdmnsync-target-p-XX`
  en todos sus comandos de `scripts/cargo.sh` o `build.sh`, para no pisar compilaciones de otros
  agentes. La primera compilación de cada volumen es completa (unos minutos).
- **Redmines de prueba**: son compartidos. Los agentes pueden leerlos, pero el test de extremo a
  extremo debe ejecutarlo **un solo agente cada vez** (o el agente principal al integrar).

### Encargo a cada agente

El agente parte sin contexto: el encargo debe ser autocontenido. Plantilla:

```
Implementa la propuesta P-XX de PROPUESTAS.md («<título>») en este worktree.
- Lee CLAUDE.md (reglas, compilación en Docker, convenciones) y la sección P-XX de PROPUESTAS.md.
- Trabaja en la rama propuesta/P-XX-<slug>.
- Usa RDMNSYNC_TARGET_VOLUME=rdmnsync-target-p-XX en todos los comandos de compilación.
- Alcance: solo P-XX. Si encuentras algo fuera de alcance, anótalo en tu informe; no lo implementes.
- Hecho cuando: compila sin warnings, los tests pasan, hay tests nuevos para la lógica añadida,
  MANUAL.md/README.md reflejan el cambio visible y PROPUESTAS.md marca P-XX como «✅ Implementada
  (rama propuesta/P-XX-…)». Haz commit en la rama (no hagas push ni merge).
- Informe final: qué cambiaste (ficheros), cómo lo verificaste, riesgos y lo que no pudiste probar.
```

### Qué propuestas pueden ir en paralelo

Dos agentes que tocan los mismos ficheros generan conflictos al integrar. Agrupa así y lanza en
paralelo **como mucho una propuesta por grupo**:

| Grupo | Ficheros principales | Propuestas |
|---|---|---|
| Sincronización (UI) | `ui/sync_view.rs`, `sync.rs` | P-05, P-06, P-07, P-09, P-13, P-17, P-22, P-32 |
| Igualado de issues | `issue_sync.rs` | P-03, P-14, P-15 |
| Mapeos y datos | `store.rs`, `ui/mappings.rs` | P-04, P-18, P-19, P-20, P-21 |
| Cliente y red | `redmine/client.rs`, `worker.rs` | P-02, P-27, P-28, P-29, P-30, P-31 |
| Emparejamiento | `matching.rs` | P-10, P-11, P-12 |
| Interfaz general | `app.rs`, `theme.rs`, `ui/settings.rs` | P-23, P-24, P-25, P-26, P-36 |
| Infraestructura | `.github/`, `docker/`, `scripts/`, `build.rs`, docs | P-33, P-34, P-35, P-37, P-38, P-40, P-42 |
| Arquitectura | todo el árbol | P-41 (en solitario: reorganiza módulos), P-43, P-44, P-01 |

Notas:
- P-41 mueve casi todos los ficheros: hazla sin ningún otro agente activo, o al principio de una
  versión.
- P-14 añade datos al historial (`store.rs`), así que coordínala con el grupo «Mapeos y datos».
- Las propuestas de infraestructura apenas tocan `src/` y son buenas candidatas para ir en paralelo
  con cualquier otra.

### Integración (agente principal)

1. Revisa el informe y el diff de cada rama (`git diff main...propuesta/P-XX-…`).
2. Integra de una en una en `main` (`git merge --no-ff`), resolviendo conflictos, y tras cada una
   ejecuta `scripts/cargo.sh test`. Al final, ejecuta una vez el test de extremo a extremo.
3. Para cambios visibles, revisa la interfaz con el piloto automático y `PrintWindow`.
4. Informa al usuario de lo integrado y lo pendiente. Push solo si lo pide.
5. Limpieza: `git worktree remove`, borrar las ramas integradas y los volúmenes
   `docker volume rm rdmnsync-target-p-XX`.
