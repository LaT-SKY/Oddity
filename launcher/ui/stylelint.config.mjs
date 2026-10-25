export default {
  extends: ["stylelint-config-recommended"],
  rules: {
    // 排版交给 Prettier, 这里只检查语法错误和可疑写法。
    // 组件状态规则按书写顺序覆盖, 不对整个文件做特异性排序。
    "no-descending-specificity": null,
    "color-no-invalid-hex": true,
    "length-zero-no-unit": true,
    "number-max-precision": 4,
    "shorthand-property-no-redundant-values": true,
    "declaration-block-no-redundant-longhand-properties": true,
  },
  overrides: [
    {
      files: ["**/*.vue"],
      customSyntax: "postcss-html",
    },
  ],
  ignoreFiles: [
    "coverage/**",
    "dist/**",
    "node_modules/**",
    "playwright-report/**",
    "test-results/**",
  ],
};
