import { z } from "zod";

// Alias schema
const AliasSchema = z.object({
  address: z.string(),
  open_url: z.string(),
});

// Resource schema
const ResourceSchema = z.object({
  address: z.string(),
  admin_url: z.string(),
  alias: z.string().optional(),
  aliases: z.array(AliasSchema).optional(),
  auth_expires_at: z.number(),
  auth_flow_id: z.string(),
  auth_state: z.string(),
  can_open_in_browser: z.boolean(),
  client_visibility: z.number(),
  id: z.string(),
  name: z.string(),
  open_url: z.string(),
  type: z.string(),
});

// Internet security schema
const InternetSecuritySchema = z.object({
  mode: z.number(),
  status: z.number(),
});

// User schema
const UserSchema = z.object({
  avatar_url: z.string(),
  email: z.string(),
  first_name: z.string(),
  id: z.string(),
  is_admin: z.boolean(),
  last_name: z.string(),
});

// Root schema
const TwingateSchema = z.object({
  admin_url: z.string(),
  full_tunnel_time_limit: z.number(),
  internet_security: InternetSecuritySchema,
  resources: z.array(ResourceSchema),
  user: UserSchema,
});

// Type inferences
export type Alias = z.infer<typeof AliasSchema>;
export type Resource = z.infer<typeof ResourceSchema>;
export type InternetSecurity = z.infer<typeof InternetSecuritySchema>;
export type User = z.infer<typeof UserSchema>;
export type TwingateConfig = z.infer<typeof TwingateSchema>;

export default TwingateSchema;
