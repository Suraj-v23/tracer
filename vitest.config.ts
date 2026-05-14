import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
  resolve: {
    alias: {
      // Allow vitest to resolve .js imports to .ts source files
      '../api/api.js':              path.resolve(__dirname, 'frontend/src/api/api.ts'),
      '../api/graph.js':            path.resolve(__dirname, 'frontend/src/api/graph.ts'),
      '../core/state.js':           path.resolve(__dirname, 'frontend/src/core/state.ts'),
      '../core/store.js':           path.resolve(__dirname, 'frontend/src/core/store.ts'),
      '../core/types.js':           path.resolve(__dirname, 'frontend/src/core/types.ts'),
      '../components/navigation.js': path.resolve(__dirname, 'frontend/src/components/navigation.ts'),
      '../utils/utils.js':          path.resolve(__dirname, 'frontend/src/utils/utils.ts'),
      '../utils/icons.js':          path.resolve(__dirname, 'frontend/src/utils/icons.ts'),
    },
  },
  test: {
    environment: 'node',
    include: ['frontend/tests/**/*.test.ts'],
  },
});
