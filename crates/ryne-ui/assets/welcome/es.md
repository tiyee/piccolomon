# Bienvenido a Ryne

Este documento es una guía editable de los formatos disponibles actualmente. Haz clic en el texto para editarlo. Los marcadores Markdown en línea aparecen alrededor del cursor y se pliegan al salir. Pasar el puntero sobre un encabezado no muestra sus marcadores.

El menú Ryne → Idioma de la barra de menús cambia la interfaz y esta guía predeterminada sin editar. Después de editarla o guardarla, pasa a ser tu documento y los cambios de idioma conservan su contenido, al igual que el de los archivos abiertos.

## Párrafos y saltos de línea

Intro crea un párrafo. Mayús+Intro inserta un salto de línea forzado. Los párrafos largos se ajustan al ancho de la ventana sin modificar el código Markdown.
Este es un salto en el código fuente del mismo párrafo.  
Esta línea sigue a un salto forzado, escrito con dos espacios antes del salto de línea.

## Encabezados

El título del documento es H1 y el de esta sección es H2. Escribe de uno a seis `#` al inicio de un bloque y luego el texto. Si falta el espacio separador, se añade automáticamente. Haz clic en un encabezado para editar sus marcadores; sal de él para plegarlos.

### H3 — Encabezado de tercer nivel

#### H4 — Encabezado de cuarto nivel

##### H5 — Encabezado de quinto nivel

###### H6 — Encabezado de sexto nivel

Encabezado Setext H1
=================

Encabezado Setext H2
-----------------

Setext usa una línea de `=` para H1 o de `-` para H2. Haz clic para mostrar y editar el subrayado.

## Formatos en línea, enlaces y escapes

**Negrita** · *Cursiva* · ~~Tachado~~ · __Negrita con guiones bajos__ · _Cursiva con guiones bajos_ · **Negrita con *cursiva anidada*** · [Enlace de ejemplo](https://example.org).

Haz clic en un formato o entra con las flechas para editar su código, incluidos los marcadores exteriores necesarios. Tras un marcador de apertura, el primer carácter del texto completa un cierre válido. Escribir ese mismo carácter permite saltar el cierre añadido. También funciona con enlaces, código en línea y fórmulas con un dólar. El código y los modos de código fuente mantienen la entrada literal.

Código en línea: `**texto literal**` y ``un `acento grave` dentro del código``. El Markdown dentro del código no se interpreta. La barra inversa conserva los signos literales: \*sin cursiva\* y \[sin enlace\].

## Listas y tareas

- Elemento sin ordenar con **negrita**.
  - Elemento anidado y fórmula: $a^2 + b^2 = c^2$.
  - Otro elemento hijo.
- Otro elemento principal.  
  El salto forzado permanece dentro de este elemento.

1. Elemento numerado.
2. Otro elemento numerado.
   1. Elemento numerado anidado.

- [ ] Una tarea pendiente.
- [x] Una tarea terminada.

Intro continúa un elemento con contenido; en uno vacío sale de un nivel. Tab / Mayús+Tab aumentan o reducen la sangría del elemento y sus hijos; el primer elemento de un nivel no puede aumentar su sangría. Mayús+Intro añade un salto dentro del elemento. Haz clic en la casilla de una tarea para cambiar su estado; puedes deshacer y rehacer el cambio. También puedes editar sus marcadores en el código del bloque.

Pasa el puntero o sitúa el cursor en un elemento para mostrar su menú de tres puntos: insertar, mover, cambiar la sangría o eliminar el elemento y sus hijos. El botón de tipo de la esquina superior derecha selecciona o copia toda la lista. Alt+Intro abre el menú del elemento activo.

## Bloques de código

Se admiten cercas de acentos graves, de virgulillas y código con cuatro espacios de sangría. Las cercas se pliegan en una zona de código con una etiqueta de lenguaje. Intro conserva la sangría y permanece dentro del bloque. Escribe la cerca inicial y un lenguaje opcional; Intro añade la cerca de cierre en una sola edición que se puede deshacer. Edita las cercas y el lenguaje desde el código del bloque.

```rust
fn main() {
    println!("Hello, Ryne!");
}
```

~~~python
def greet(name):
    return f"Hello, {name}!"
~~~

    **Markdown literal en código con sangría**
    [el enlace también es literal](https://example.org)

- El código también puede estar dentro de una lista:
  ```json
  {"editor": "Ryne", "ready": true}
  ```

El resaltado admite Rust, Python, JavaScript/JSX, TypeScript/TSX, JSON y Bash, con alias como `rs`, `py`, `js`, `ts` y `sh`. Pegar código válido y distintivo o insertar un salto en el código puede completar un lenguaje vacío. Los lenguajes desconocidos y el código ambiguo mantienen un estilo sencillo; se conserva un lenguaje ya indicado.

## Diagramas Mermaid

```mermaid
flowchart LR
    A[Abrir] --> B[Editar] --> C[Guardar]
```

Sal de un bloque `mermaid` cerrado para ver el diagrama. Haz clic en la vista previa o entra con las flechas para editar el mismo código; Esc lo pliega de nuevo. Los diagramas inválidos, incompletos o demasiado grandes siguen siendo código editable con un diagnóstico.

## Fórmulas en línea

Energía: $E = mc^2$. Fracciones y subíndices: $\frac{a_1}{\sqrt{b}}$.

Coloca un solo `$` a cada lado del LaTeX de una línea. Haz clic o entra con las flechas para editar el código; sal para ver la fórmula. También funciona en listas y celdas de tabla. Las fórmulas independientes (`$$...$$`), de varias líneas, con glifos ausentes o no compatibles conservan su código.

## Tablas

| Alineado a la izquierda | Centrado | Alineado a la derecha |
| :--- | :---: | ---: |
| **Celda en negrita** | `code` | 123 |
| Primera línea<br>Segunda línea | Barra escapada: a\|b | $x^2$ |
| Vecina vacía | | 456 |

Haz clic en una celda para editarla. Tab / Mayús+Tab cambian de celda; Intro baja por la misma columna. Avanzar más allá de la última fila puede añadir otra. Mayús+Intro añade `<br>` dentro de la celda. Las barras verticales escritas o pegadas se escapan automáticamente.

El menú de tres puntos de la celda cambia la alineación e inserta, mueve o elimina filas y columnas. Alt+Intro abre el menú activo; las flechas, Intro y Esc permiten navegar. Edita los separadores desde el código del bloque. Las tablas con distinto número de columnas conservan el código. Aún no se admite ajustar el ancho de columnas ni pegar TSV en varias celdas.

## Citas, imágenes, separadores y texto sin procesar

Estas formas se pueden insertar y editar como código Markdown; actualmente sus marcadores estructurales permanecen visibles. Las citas aún no tienen barra visual ni continuación automática. Las imágenes aún no tienen vista previa. Intro inserta un salto literal en estos bloques de código fuente.

> Una cita con **negrita en línea**.
> > Una cita anidada.

![Descripción de la imagen](https://example.org/image.png)

---

<div>El HTML sin procesar permanece como código editable.</div>

El `---` anterior es un separador en su propia línea, rodeado de líneas vacías. La sintaxis incompleta o no compatible sigue siendo editable sin perder contenido.

## Código fuente, inserción y archivos

Pasa el puntero sobre el botón de tipo de un bloque para editar su código local, seleccionar el bloque o copiar el código. El código de bloques anidados incluye los prefijos del contenedor. Cmd+Mayús+B alterna el código del bloque; Cmd+Mayús+M el código completo. Esc cierra los menús o pliega el código. En modo de código fuente, la entrada y los saltos son literales.

Al final del menú de tipo de bloque, elige «Añadir debajo» o «Añadir encima» y después un tipo de bloque. El primer bloque solo ofrece «Añadir debajo». La inserción se deshace en un paso; Esc vuelve del selector al menú del bloque.

Pasa el puntero debajo del documento para mostrar `+` e insertar estos tipos de bloques. Completa primero las estructuras sin cerrar antes de añadir un bloque.

Cmd+O abre un archivo; Cmd+S guarda; Cmd+Mayús+S guarda como. Cmd+Z / Cmd+Mayús+Z deshacen y rehacen. Cmd+A/C/X/V seleccionan todo, copian el código, cortan y pegan. La selección y el pegado pueden abarcar líneas y bloques. Ver marcadores, menús o código no modifica el documento; al guardar se conservan los bytes originales y los finales de línea. La confirmación del IME se aplica como una sola edición que se puede deshacer.

Pulsa Formatear en la barra de herramientas para dar formato a toda la fuente Markdown en una operación que puedes deshacer. Las reglas añaden los espacios que faltan en títulos aislados y marcadores de tareas, conservando el código, los saltos de párrafo y la sangría de los contenedores. Guardar no aplica formato automáticamente.
