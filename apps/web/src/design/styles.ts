// Typed, named parts from the application's multi-part recipes.
// Component definitions and variants live in theme/recipes.
import {
  conditions,
  data,
  canvas,
  catalogue,
  dashboard,
  runDebugger,
  dialog,
  editor,
  feedback,
  inspector,
  login,
  menu,
  palette,
  settings,
  shared,
  shell
} from '../../styled-system/recipes'

export const styles = {
  ...conditions(),
  ...data(),
  ...canvas(),
  ...catalogue(),
  ...dashboard(),
  ...runDebugger(),
  ...dialog(),
  ...editor(),
  ...feedback(),
  ...inspector(),
  ...login(),
  ...menu(),
  ...palette(),
  ...settings(),
  ...shared(),
  ...shell()
}
