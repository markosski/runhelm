import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'RunHelm',
      logo: {
        src: './public/runhelm-logo.png',
        alt: 'RunHelm',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/markosski/runhelm',
        },
      ],
      components: {
        Head: './src/components/Head.astro',
      },
      sidebar: [
        {
          label: 'Start',
          items: [
            { label: 'Overview', slug: 'docs' },
            { label: 'Install', slug: 'docs/install' },
            { label: 'API Reference', slug: 'docs/api-reference' },
          ],
        },
        {
          label: 'Concepts',
          items: [
            { label: 'Workflows', slug: 'docs/concepts/workflows' },
            {
              label: 'Tasks',
              items: [
                { label: 'Overview', slug: 'docs/concepts/tasks' },
                { label: 'Agent Tasks', slug: 'docs/concepts/tasks/agents' },
                { label: 'Function Tasks', slug: 'docs/concepts/tasks/functions' },
                { label: 'API Call Tasks', slug: 'docs/concepts/tasks/api-calls' },
              ],
            },
            { label: 'Bounded Loops', slug: 'docs/concepts/bounded-loops' },
            { label: 'Human Input', slug: 'docs/concepts/human-input' },
            { label: 'Architecture', slug: 'docs/concepts/architecture' },
          ],
        },
        {
          label: 'Operations',
          items: [
            { label: 'Workspaces', slug: 'docs/operations/workspaces' },
            { label: 'Credentials', slug: 'docs/operations/credentials' },
          ],
        },
        {
          label: 'Examples',
          items: [
            {
              label: 'GitHub Issue to PR',
              slug: 'docs/examples/github-issue-pr-workflow',
            },
          ],
        },
      ],
    }),
  ],
});
