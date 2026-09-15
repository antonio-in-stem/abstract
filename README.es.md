<p align="center"><img src="editors/vscode/icons/abstract-logo.png" width="96" alt="Abstract"></p>

# Abstract

**Define tus objetos. Escribe sus variantes.**

Abstract es un lenguaje de datos para definir familias de objetos, escribir sus
instancias y comprobar las reglas que comparten. El compilador resuelve el
proyecto y genera JSON, YAML o RAW. Tu aplicación decide qué hacer con esos datos.

Una definición `.abt` establece las posibilidades: estructura, valores admitidos,
límites, referencias y recursos. Un archivo `.ab` aporta objetos concretos. Una
variante puede reutilizar datos de otra instancia y escribir solo sus diferencias.

## Empieza con un problema pequeño

Los [seis ejercicios](docs/learn/README.md) incluyen enunciado, pista y una solución
explicada por separado. Empiezan con un esquema sencillo y avanzan hacia reglas,
composición, versiones y aritmética. El [catálogo de automóviles](examples/automotive)
queda como ejemplo avanzado.

Para comparar enfoques, el [mismo problema en Abstract y CUE](docs/comparison/README.md)
incluye ambos programas, sus resultados y comprobaciones de errores.

## Instalar y escribir

Descarga el compilador y el archivo VSIX de la extensión desde
[Releases](https://github.com/antonio-in-stem/abstract/releases). En VS Code,
ejecuta **Extensions: Install from VSIX** y configura `abstract.compilerPath`
si el compilador no está en PATH. La extensión es de **Antonio M.**

Con el ejecutable en PATH puedes empezar sin instalar Rust:

```sh
abstract init mi-proyecto
abstract compile mi-proyecto JSON --out mi-proyecto.json
```

También puedes compilar desde el código:

```sh
cargo build --release --locked
cargo run -- init mi-proyecto
cargo run -- compile mi-proyecto JSON --out mi-proyecto.json
```

## Lo que comprueba y lo que entrega

Abstract valida tipos, reglas, referencias y recursos declarados. Sus comprobaciones
de imágenes leen cabeceras y dimensiones; no decodifican todos los píxeles.
`file(json)` declara un recurso, no un esquema para el contenido de ese JSON.
La salida ordinaria conserva rutas, no empaqueta los bytes de los archivos.

Los overlays representan estados de un modelo en distintas versiones. No son un
historial de Git ni migraciones de una base de datos. La aritmética calcula datos
durante la compilación; no añade comportamiento ejecutable a tus objetos.

La utilidad está en mantener lo común y expresar las diferencias sin perder las
reglas. Para una configuración pequeña puede bastar con JSON o YAML. CUE también
ofrece estructuras reutilizables y restricciones; Abstract propone un flujo
específico de identidad, clonación, recursos y versiones.

## Proyecto

Compilador **1.5.0**, lenguaje **1.2**, extensión **1.7.3**. Los tres tienen versiones
independientes. La [guía de release](docs/RELEASE.md) recoge verificaciones y límites.
La [guía de compatibilidad](docs/COMPATIBILITY.md) explica los contratos y la
migración de bundles antiguos. La [documentación principal](README.md) enlaza
la especificación.

El código se distribuye bajo [MIT](LICENSE), conservando los avisos de autoría.
El nombre y el logo de Abstract siguen siendo propiedad de Antonio M.
