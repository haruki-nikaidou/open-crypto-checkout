// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	integrations: [
		starlight({
			title: 'Open Crypto Checkout',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/haruki-nikaidou/open-crypto-checkout' }],
			sidebar: [
				{
					label: 'Quick Start',
					items: [
						{ label: 'Introduction', slug: 'quick-start/introduction' },
						{ label: 'Deploy with Docker', slug: 'quick-start/docker' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Configuration', slug: 'guides/configuration' },
						{ label: 'Deploy with systemd', slug: 'guides/deploy-systemd' },
						{ label: 'Frontend Development', slug: 'guides/frontend' },
						{ label: 'Webhooks', slug: 'guides/webhooks' },
					],
				},
				{
					label: 'API Reference',
					items: [
						{ label: 'Authentication', slug: 'reference/authentication' },
						{ label: 'Service API', slug: 'reference/service-api' },
						{ label: 'User API', slug: 'reference/user-api' },
						{ label: 'Admin API', slug: 'reference/admin-api' },
					],
				},
			],
		}),
	],
});
