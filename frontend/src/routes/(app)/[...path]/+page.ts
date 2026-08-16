import { redirect } from '@sveltejs/kit';

import type { PageLoad } from './$types';

export const prerender = false;

const sectionEntries: Record<string, string> = {
  discovery: '/discovery/subscriptions',
  system: '/system/account'
};

export const load: PageLoad = ({ params }) => {
  const target = sectionEntries[params.path ?? ''];
  if (target) redirect(307, target);
};
