# Propuestas para próximas versiones

Este documento complementa la [hoja de ruta del README](README.md#hoja-de-ruta). La hoja de ruta
recoge los compromisos ya identificados; aquí se desarrollan esos puntos y se proponen funcionalidades
y mejoras adicionales, con su motivación, un esbozo de solución y una estimación de valor y esfuerzo
para ayudar a priorizar.

Muchas propuestas parten de limitaciones reales de la versión 0.1.0 detectadas al construirla y
probarla; se indican como tales.

## Contenido

1. [Cómo leer este documento](#1-cómo-leer-este-documento)
2. [Desarrollo de la hoja de ruta](#2-desarrollo-de-la-hoja-de-ruta)
3. [Sincronización de imputaciones](#3-sincronización-de-imputaciones)
4. [Emparejamiento de issues](#4-emparejamiento-de-issues)
5. [Igualado y creación de issues](#5-igualado-y-creación-de-issues)
6. [Mapeos y conocimiento compartido](#6-mapeos-y-conocimiento-compartido)
7. [Experiencia de uso](#7-experiencia-de-uso)
8. [Robustez y rendimiento](#8-robustez-y-rendimiento)
9. [Seguridad](#9-seguridad)
10. [Distribución y actualizaciones](#10-distribución-y-actualizaciones)
11. [Calidad y mantenimiento](#11-calidad-y-mantenimiento)
12. [Ideas a más largo plazo](#12-ideas-a-más-largo-plazo)
13. [Propuesta de planificación por versiones](#13-propuesta-de-planificación-por-versiones)

---

## 1. Cómo leer este documento

Cada propuesta lleva un identificador (`P-xx`) para poder referirse a ella en issues y commits, y dos
estimaciones orientativas:

| Valor | Significado |
|---|---|
| ⭐⭐⭐ | Resuelve un problema frecuente o un riesgo real. |
| ⭐⭐ | Mejora notable para parte de los usuarios. |
| ⭐ | Conveniente, pero prescindible. |

| Esfuerzo | Orientación |
|---|---|
| **S** | Horas; cambio localizado. |
| **M** | Días; toca varios módulos. |
| **L** | Semanas; requiere diseño previo. |

---

## 2. Desarrollo de la hoja de ruta

### P-01 · Migrar imputaciones de otros usuarios (perfiles de administración)
**Valor** ⭐⭐ · **Esfuerzo** L

Hoy solo se leen y crean las imputaciones del dueño de la API key.

- Añadir un selector de **usuario de origen** (o «todos») visible solo si la API key tiene permisos
  para ver imputaciones ajenas.
- Añadir un **mapeo de usuarios** origen → destino (por login, email o nombre, con confirmación,
  igual que las actividades).
- En el destino, crear con `user_id` del usuario mapeado, lo que exige el permiso «Imputar tiempo
  para otros usuarios». Comprobarlo antes de enviar y explicarlo si falta.
- Mostrar en la tabla de imputaciones una columna de usuario y permitir filtrar por él.
- **Riesgo**: un error afecta a las horas de otras personas. Conviene exigir una confirmación
  explícita adicional y registrar en el historial quién hizo cada envío.

### P-02 · Crear versiones en el destino
**Valor** ⭐⭐ · **Esfuerzo** S

Ahora, si una versión no existe en el destino, el usuario debe crearla en la web y volver a analizar.

- Botón **Crear en el destino** junto a cada versión sin equivalente, que haga
  `POST /projects/:id/versions` copiando nombre, descripción, fecha prevista y estado.
- Guardar el mapeo resultante y recalcular las diferencias sin volver a analizar.
- Requiere el permiso «Gestionar versiones»; si falta, mostrar el mensaje de Redmine.

### P-03 · Igualar cualquier issue enlazado
**Valor** ⭐⭐ · **Esfuerzo** M

Hoy solo se igualan los issues que tienen imputaciones en el periodo analizado.

- Nueva acción en **Mapeos → Issues enlazados**: «Revisar cambios» sobre una selección o sobre todos.
- Reutilizar `issue_sync::diff` y la misma vista previa de cambios de la pestaña Sincronizar.
- Útil para cerrar en el destino los issues que se terminaron en el origen sin imputar más horas.

### P-04 · Exportar el historial a CSV
**Valor** ⭐ · **Esfuerzo** S

- Botón en **Mapeos → Historial**: exportar una o todas las sincronizaciones con fecha, horas,
  actividad, issue de origen y de destino, e ids de imputación.
- Separador `;` y BOM UTF-8 para que Excel lo abra bien en configuración regional española.

---

## 3. Sincronización de imputaciones

### P-05 · Propagar ediciones y borrados del origen
**Valor** ⭐⭐⭐ · **Esfuerzo** M

Si una imputación ya sincronizada se corrige en el origen (otras horas, otro comentario u otra
actividad), hoy sigue apareciendo como «Ya sincronizada» y el destino queda desfasado. Si se borra en
el origen, la copia del destino permanece.

- Comparar cada imputación «Ya sincronizada» con su copia en el destino (localizada por la marca o el
  historial) y mostrar un estado nuevo, **«Modificada en origen»**, con las diferencias.
- Ofrecer **Actualizar** (`PUT /time_entries/:id`) o **Ignorar**.
- Para las del historial cuyo original ya no existe, mostrar **«Borrada en origen»** con opción de
  borrar la copia. Nunca hacerlo automáticamente.

### P-06 · Editar antes de enviar
**Valor** ⭐⭐ · **Esfuerzo** S

Permitir ajustar en la propia tabla las horas, el comentario o la actividad de una imputación solo
para el destino, sin tocar el origen. Útil cuando los criterios de imputación difieren entre empresas.

### P-07 · Reglas de transformación de horas
**Valor** ⭐⭐ · **Esfuerzo** M

Algunos Redmines exigen granularidad (cuartos de hora, mínimos por imputación) o agrupación diaria.

- **Redondeo** configurable por pareja: al cuarto, a la media hora, hacia arriba, hacia abajo o al
  más cercano.
- **Agrupación** opcional: una imputación por día, issue y actividad, concatenando comentarios.
- Mostrar siempre en la tabla las horas originales y las resultantes.

### P-08 · Periodo por defecto y recordatorios
**Valor** ⭐ · **Esfuerzo** S

- Recordar el último periodo y la última pareja usados.
- Aviso al abrir la app si la semana anterior tiene diferencias en el Resumen semanal.

### P-09 · Comprobaciones previas al envío
**Valor** ⭐⭐ · **Esfuerzo** M

Hoy algunos rechazos solo se descubren al enviar: actividad deshabilitada en el proyecto, issue
cerrado que no admite imputaciones o periodo bloqueado por un plugin.

- Al analizar, cargar las actividades de cada proyecto de destino implicado y marcar como bloqueadas
  las filas cuya actividad no esté habilitada, con su motivo.
- Avisar si el issue de destino está cerrado.

---

## 4. Emparejamiento de issues

### P-10 · Emparejar por campo personalizado o referencia en el asunto
**Valor** ⭐⭐⭐ · **Esfuerzo** M

En muchas organizaciones el issue de destino guarda la referencia del origen en un campo
personalizado («Ticket cliente») o en el asunto («[CLI-1234] …»).

- Configuración por pareja: «el campo personalizado *X* del destino contiene el id del origen» o
  «el asunto del destino contiene `[<prefijo>-<id>]`».
- Tratarlo como referencia explícita (100 %).
- Al crear issues desde la app, rellenar automáticamente ese campo o ese prefijo.

### P-11 · Explicar y ajustar la puntuación
**Valor** ⭐ · **Esfuerzo** S

- Desglose visible al pasar el ratón: cuánto aporta el asunto, el proyecto y la asignación.
- Pesos configurables en Ajustes para casos atípicos.

### P-12 · Aprender de las correcciones
**Valor** ⭐⭐ · **Esfuerzo** M

Cuando el usuario rechaza un enlace automático y elige otro, registrar ese «falso positivo» y no
volver a proponerlo para el mismo par. Con suficientes correcciones, ajustar el umbral automático de
esa pareja.

### P-13 · Buscador de issues en el destino
**Valor** ⭐⭐ · **Esfuerzo** S

Además de escribir un número, poder **buscar por texto** dentro de la tarjeta de «Issues por
confirmar», con resultados en vivo. Hoy hay que conocer el número o abrir la web.

---

## 5. Igualado y creación de issues

### P-14 · Deshacer el igualado de issues
**Valor** ⭐⭐⭐ · **Esfuerzo** S

Limitación actual: **Deshacer** borra imputaciones, pero no revierte los cambios aplicados a los
issues de destino.

- El cálculo de diferencias ya conoce el valor anterior de cada campo. Guardarlo en el historial
  junto al envío.
- Al deshacer, restaurar esos valores, salvo los campos que se hayan vuelto a modificar en el
  destino, que se listan para que el usuario decida.

### P-15 · Sincronizar notas (journals)
**Valor** ⭐⭐ · **Esfuerzo** M

Opción para copiar al destino, como notas, los comentarios añadidos en el origen desde la última
sincronización, con una cabecera «Nota de <autor> en <origen>». Desactivada por defecto, por la
posible información confidencial.

### P-16 · Plantillas de creación de issues
**Valor** ⭐ · **Esfuerzo** S

Valores por defecto por proyecto de destino (tipo, categoría, campos personalizados fijos) para no
rellenarlos cada vez en el diálogo de creación.

### P-17 · Crear varios issues de una vez
**Valor** ⭐⭐ · **Esfuerzo** M

Cuando hay muchos issues «Sin equivalente claro», permitir seleccionarlos y crearlos todos con los
valores propuestos, mostrando solo los que fallen para su corrección manual.

---

## 6. Mapeos y conocimiento compartido

### P-18 · Exportar e importar mapeos
**Valor** ⭐⭐⭐ · **Esfuerzo** S

Los mapeos son conocimiento valioso para todo un equipo que trabaja con los mismos Redmines.

- Exportar los mapeos de una pareja a JSON (sin API keys ni historial).
- Importar con vista previa y resolución de conflictos (mantener, sustituir o fusionar).
- Identificar los Redmines por URL al importar.

### P-19 · Editar tipos y prioridades en Mapeos
**Valor** ⭐ · **Esfuerzo** S

Los mapeos de tipos (*trackers*) y prioridades ya se guardan al crear issues, pero no tienen sección
en la pestaña Mapeos. Añadirla, igual que la de Estados.

### P-20 · Mapeos de actividad por proyecto
**Valor** ⭐⭐ · **Esfuerzo** M

Un mismo nombre de actividad puede necesitar destinos distintos según el proyecto («Desarrollo» →
«Codificar» en un proyecto y → «Evolutivo» en otro). Permitir excepciones por proyecto de destino
sobre el mapeo general.

### P-21 · Reglas más expresivas
**Valor** ⭐ · **Esfuerzo** M

Ampliar las reglas por comentario con condiciones sobre proyecto, issue o actividad de origen, y con
acciones adicionales: fijar el issue o el proyecto de destino (útil para imputaciones genéricas como
«Formación» o «Gestión» que van siempre al mismo issue interno).

---

## 7. Experiencia de uso

### P-22 · Selector de fechas visual
**Valor** ⭐⭐ · **Esfuerzo** S

Sustituir los campos de texto `AAAA-MM-DD` por un calendario desplegable (egui_extras lo incluye con
la *feature* `datepicker`).

### P-23 · Idioma inglés
**Valor** ⭐⭐ · **Esfuerzo** M

Extraer los textos de la interfaz a un catálogo y añadir inglés, con selección automática según el
sistema. Las familias de sinónimos de actividades y estados ya contemplan nombres en inglés.

### P-24 · Accesibilidad
**Valor** ⭐⭐ · **Esfuerzo** S

La *feature* `accesskit` de eframe está desactivada para reducir el tamaño del binario, lo que impide
usar la app con lectores de pantalla. Activarla, revisar el orden de foco y los textos de los botones
que solo tienen icono, y añadir atajos de teclado (Ctrl+Intro para analizar, Ctrl+S para enviar…).

### P-25 · Recordar el estado de la interfaz
**Valor** ⭐ · **Esfuerzo** S

Recordar la pestaña activa, el Redmine y el proyecto del Explorador, los filtros y las secciones
plegadas entre sesiones.

### P-26 · Estadísticas e informes
**Valor** ⭐ · **Esfuerzo** M

Ampliar el Resumen semanal con vista mensual, desglose por proyecto y actividad, y gráficas simples
de horas por semana en cada Redmine.

---

## 8. Robustez y rendimiento

### P-27 · Peticiones en paralelo y caché
**Valor** ⭐⭐ · **Esfuerzo** M

El análisis hace las peticiones al destino de forma secuencial (issues asignados, por proyecto,
búsquedas por palabra clave…). Con muchos issues puede tardar.

- Paralelizar las peticiones independientes con un límite de concurrencia (p. ej. 4) para no
  sobrecargar el servidor.
- Cachear durante la sesión los datos que cambian poco (proyectos, actividades, estados, versiones)
  con un botón de refresco.

### P-28 · Reintentos y límites del servidor
**Valor** ⭐⭐ · **Esfuerzo** S

Reintentar con espera exponencial ante errores 429, 502, 503 y 504 y fallos de red transitorios,
especialmente durante el envío, para no dejar lotes a medias por un corte puntual.

### P-29 · Registro de diagnóstico
**Valor** ⭐⭐ · **Esfuerzo** S

No hay registro de actividad, lo que dificulta diagnosticar problemas en entornos ajenos.

- Fichero de log rotativo junto a la configuración, con nivel configurable.
- **Nunca** registrar API keys; enmascarar cabeceras y URLs que puedan contenerlas.
- Botón «Copiar información de diagnóstico» en Ajustes: versión, sistema, versiones de Redmine y
  últimos errores.

### P-30 · Detección de la versión de Redmine
**Valor** ⭐ · **Esfuerzo** S

Consultar la versión de cada servidor al conectar, mostrarla y adaptar el comportamiento (por
ejemplo, la inclusión de actividades por proyecto no existe en versiones antiguas).

---

## 9. Seguridad

### P-31 · Certificados raíz personalizados
**Valor** ⭐⭐ · **Esfuerzo** S

La opción «Aceptar certificados no válidos» desactiva por completo la verificación TLS. Es mejor
permitir **añadir un certificado de CA** (fichero PEM) por Redmine y mantener la verificación activa,
dejando la desactivación como último recurso con un aviso más visible.

### P-32 · Confirmaciones proporcionales al riesgo
**Valor** ⭐⭐ · **Esfuerzo** S

Pedir confirmación explícita antes de envíos grandes (p. ej. más de 40 imputaciones o más de 20
issues a actualizar) y antes de **Deshacer**, mostrando exactamente qué se va a crear, modificar o
borrar.

### P-33 · Firma de binarios
**Valor** ⭐⭐ · **Esfuerzo** M

Firmar el ejecutable de Windows (Authenticode) para evitar el aviso de SmartScreen y permitir
comprobar su origen. Requiere un certificado de firma de código y un paso adicional en la compilación.

---

## 10. Distribución y actualizaciones

### P-34 · Integración continua y releases automáticas
**Valor** ⭐⭐⭐ · **Esfuerzo** M

- GitHub Actions que, en cada *push* y *pull request*, ejecute los tests unitarios y el test de
  extremo a extremo levantando los Redmines de prueba con Docker Compose.
- Al crear una etiqueta `v*`, compilar con la misma imagen Docker y publicar la release con binarios
  y sumas SHA-256, sin pasos manuales.

### P-35 · Icono del ejecutable y metadatos
**Valor** ⭐ · **Esfuerzo** S

Hoy el icono solo aparece en la ventana; el `.exe` no lleva icono ni información de versión. Incrustar
ambos con un `build.rs` (`winresource`) usando el `windres` de mingw que ya trae la imagen de
compilación.

### P-36 · Comprobación de actualizaciones
**Valor** ⭐⭐ · **Esfuerzo** S

Consultar al arrancar (con opción de desactivarlo) la última release en la API de GitHub y avisar si
hay una versión nueva, con enlace a sus notas. Sin actualización automática del binario.

### P-37 · Paquetes nativos
**Valor** ⭐ · **Esfuerzo** M

- Windows: instalador opcional (MSI o NSIS) y publicación en `winget`.
- Linux: AppImage y Flatpak, que además resuelven el acceso al servicio de secretos.

### P-38 · Versión para macOS
**Valor** ⭐ · **Esfuerzo** M

La compilación cruzada a macOS desde Docker no es viable, pero puede hacerse en un runner macOS de
GitHub Actions. El código no tiene dependencias específicas de Windows o Linux salvo el almacén de
credenciales, que `keyring` ya soporta en macOS.

---

## 11. Calidad y mantenimiento

### P-39 · Tests de interfaz
**Valor** ⭐⭐ · **Esfuerzo** M

Usar `egui_kittest` para probar flujos de la interfaz (confirmar actividades, enlazar issues, enviar)
y para generar capturas de referencia que detecten cambios visuales involuntarios.

### P-40 · Matriz de versiones de Redmine
**Valor** ⭐⭐ · **Esfuerzo** S

Ejecutar el test de extremo a extremo contra Redmine 4.2, 5.1 y 6.x para dar soporte declarado a
esas versiones. Hoy solo se ha probado con la 6.

### P-41 · Separar la interfaz del motor
**Valor** ⭐ · **Esfuerzo** M

Mover el cliente de Redmine, el emparejamiento y la sincronización a una biblioteca (`rdmnsync-core`)
dentro de un *workspace*. Facilita los tests, la línea de comandos (P-43) y futuras interfaces.

### P-42 · Documentos del proyecto
**Valor** ⭐ · **Esfuerzo** S

Añadir `LICENSE` (la licencia MIT solo está declarada en `Cargo.toml`), `CHANGELOG.md` con los
cambios de cada versión y `CONTRIBUTING.md` con cómo compilar, probar y proponer cambios.

---

## 12. Ideas a más largo plazo

### P-43 · Modo línea de comandos
**Valor** ⭐⭐ · **Esfuerzo** M

`rdmnsync sync --from cliente --to interno --period last-week` para automatizar con el programador de
tareas o cron. Solo enviaría lo que no requiera decisiones (enlaces confirmados y actividades
mapeadas) y dejaría el resto como pendiente en un informe.

### P-44 · Sincronización programada
**Valor** ⭐ · **Esfuerzo** L

Ejecución periódica en segundo plano desde la bandeja del sistema, con notificación de lo enviado y
de lo que requiere atención. Depende de P-43.

### P-45 · Otras fuentes de horas
**Valor** ⭐ · **Esfuerzo** L

Aprovechar el motor de emparejamiento y mapeo con orígenes distintos de Redmine (Jira, Toggl,
Clockify o un CSV) y Redmine como destino.

### P-46 · Sincronización en ambos sentidos
**Valor** ⭐ · **Esfuerzo** L

Hoy cada ejecución es en un solo sentido y así debe seguir por defecto. Un modo bidireccional
necesitaría detección de conflictos y una regla clara de prioridad por campo; solo merece la pena si
hay demanda real.

---

## 13. Propuesta de planificación por versiones

Agrupación orientativa por valor, riesgo y dependencias:

### 0.2 — Fiabilidad
Reducir errores al enviar y facilitar el diagnóstico.

| Propuesta | Valor | Esfuerzo |
|---|---|---|
| P-14 Deshacer el igualado de issues | ⭐⭐⭐ | S |
| P-09 Comprobaciones previas al envío | ⭐⭐ | M |
| P-28 Reintentos y límites del servidor | ⭐⭐ | S |
| P-29 Registro de diagnóstico | ⭐⭐ | S |
| P-32 Confirmaciones proporcionales al riesgo | ⭐⭐ | S |
| P-34 Integración continua y releases automáticas | ⭐⭐⭐ | M |
| P-42 Documentos del proyecto | ⭐ | S |

### 0.3 — Menos intervención manual
Que cada sincronización requiera todavía menos decisiones.

| Propuesta | Valor | Esfuerzo |
|---|---|---|
| P-05 Propagar ediciones y borrados del origen | ⭐⭐⭐ | M |
| P-10 Emparejar por campo personalizado o referencia | ⭐⭐⭐ | M |
| P-18 Exportar e importar mapeos | ⭐⭐⭐ | S |
| P-02 Crear versiones en el destino | ⭐⭐ | S |
| P-13 Buscador de issues en el destino | ⭐⭐ | S |
| P-22 Selector de fechas visual | ⭐⭐ | S |
| P-03 Igualar cualquier issue enlazado | ⭐⭐ | M |

### 0.4 — Alcance
Más usuarios y más entornos.

| Propuesta | Valor | Esfuerzo |
|---|---|---|
| P-23 Idioma inglés | ⭐⭐ | M |
| P-24 Accesibilidad | ⭐⭐ | S |
| P-27 Peticiones en paralelo y caché | ⭐⭐ | M |
| P-31 Certificados raíz personalizados | ⭐⭐ | S |
| P-35 Icono del ejecutable y metadatos | ⭐ | S |
| P-36 Comprobación de actualizaciones | ⭐⭐ | S |
| P-40 Matriz de versiones de Redmine | ⭐⭐ | S |

### 1.0 — Equipos
Uso por equipos y administración.

| Propuesta | Valor | Esfuerzo |
|---|---|---|
| P-01 Migrar imputaciones de otros usuarios | ⭐⭐ | L |
| P-41 Separar la interfaz del motor | ⭐ | M |
| P-43 Modo línea de comandos | ⭐⭐ | M |
| P-33 Firma de binarios | ⭐⭐ | M |

El resto de propuestas (P-04, P-06 a P-08, P-11, P-12, P-15 a P-17, P-19 a P-21, P-25, P-26, P-30,
P-37 a P-39 y P-44 a P-46) quedan como candidatas para incorporar según la demanda.
