/* auto-generated by NAPI-RS */
/* eslint-disable */
export interface CodegenOptions {
  /**
   * Remove whitespace.
   *
   * @default true
   */
  removeWhitespace?: boolean
}

export interface CompressOptions {
  /**
   * Set desired EcmaScript standard version for output.
   *
   * Set `esnext` to enable all target highering.
   *
   * e.g.
   *
   * * catch optional binding when >= es2019
   * * `??` operator >= es2020
   *
   * @default 'esnext'
   */
  target?: 'esnext' | 'es2015' | 'es2016' | 'es2017' | 'es2018' | 'es2019' | 'es2020' | 'es2021' | 'es2022' | 'es2023' | 'es2024'
  /**
   * Pass true to discard calls to `console.*`.
   *
   * @default false
   */
  dropConsole?: boolean
  /**
   * Remove `debugger;` statements.
   *
   * @default true
   */
  dropDebugger?: boolean
  /**
   * Drop unreferenced functions and variables.
   *
   * Simple direct variable assignments do not count as references unless set to "keep_assign".
   */
  unused?: true | false | 'keep_assign'
  /** Keep function / class names. */
  keepNames?: CompressOptionsKeepNames
}

export interface CompressOptionsKeepNames {
  /**
   * Keep function names so that `Function.prototype.name` is preserved.
   *
   * This does not guarantee that the `undefined` name is preserved.
   *
   * @default false
   */
  function: boolean
  /**
   * Keep class names so that `Class.prototype.name` is preserved.
   *
   * This does not guarantee that the `undefined` name is preserved.
   *
   * @default false
   */
  class: boolean
}

export interface MangleOptions {
  /**
   * Pass `true` to mangle names declared in the top level scope.
   *
   * @default false
   */
  toplevel?: boolean
  /**
   * Preserve `name` property for functions and classes.
   *
   * @default false
   */
  keepNames?: boolean | MangleOptionsKeepNames
  /** Debug mangled names. */
  debug?: boolean
}

export interface MangleOptionsKeepNames {
  /**
   * Preserve `name` property for functions.
   *
   * @default false
   */
  function: boolean
  /**
   * Preserve `name` property for classes.
   *
   * @default false
   */
  class: boolean
}

/** Minify synchronously. */
export declare function minify(filename: string, sourceText: string, options?: MinifyOptions | undefined | null): MinifyResult

export interface MinifyOptions {
  compress?: boolean | CompressOptions
  mangle?: boolean | MangleOptions
  codegen?: boolean | CodegenOptions
  sourcemap?: boolean
}

export interface MinifyResult {
  code: string
  map?: SourceMap
  errors: Array<OxcError>
}
export interface Comment {
  type: 'Line' | 'Block'
  value: string
  start: number
  end: number
}

export interface ErrorLabel {
  message?: string
  start: number
  end: number
}

export interface OxcError {
  severity: Severity
  message: string
  labels: Array<ErrorLabel>
  helpMessage?: string
  codeframe?: string
}

export declare const enum Severity {
  Error = 'Error',
  Warning = 'Warning',
  Advice = 'Advice'
}
export interface SourceMap {
  file?: string
  mappings: string
  names: Array<string>
  sourceRoot?: string
  sources: Array<string>
  sourcesContent?: Array<string>
  version: number
  x_google_ignoreList?: Array<number>
}
