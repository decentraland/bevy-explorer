// Matte landscape response, authored here: one diffuse light evaluation rather
// than a full metallic BRDF followed by a second corrective lighting pass.
#import bevy_pbr::{
    pbr_types::PbrInput,
    mesh_view_bindings as bindings,
    mesh_view_types,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    clustered_forward as clustering,
    lighting::getDistanceAttenuation,
    shadows,
}
#import "embedded://visuals/unity_ambient.wgsl"::unity_ambient_irradiance
#import bevy_render::maths::PI

fn landscape_lighting(input: PbrInput, foliage: f32) -> vec4<f32> {
    let normal = normalize(input.N);
    let ambient_normal = normalize(mix(normal, vec3(0.0, 1.0, 0.0), foliage * 0.4));
    var irradiance = unity_ambient_irradiance(ambient_normal) * input.diffuse_occlusion;
    let view_z = dot(vec4(bindings::view.view_from_world[0].z, bindings::view.view_from_world[1].z,
        bindings::view.view_from_world[2].z, bindings::view.view_from_world[3].z), input.world_position);
    let receives_shadow = (input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u;
    for (var i = 0u; i < bindings::lights.n_directional_lights; i += 1u) {
        let light = bindings::lights.directional_lights[i];
        let cosine = dot(normal, light.direction_to_light);
        let diffuse = max(cosine, 0.0) + foliage * max(-cosine, 0.0) * 0.22;
        if diffuse <= 0.0 { continue; }
        var shadow = 1.0;
        if receives_shadow && (light.flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_directional_shadow(i, input.world_position, input.world_normal, view_z);
        }
        irradiance += light.color.rgb * (diffuse * shadow / PI);
    }
    let cluster = clustering::fragment_cluster_index(input.frag_coord.xy, view_z, input.is_orthographic);
    let ranges = clustering::unpack_clusterable_object_index_ranges(cluster);
    for (var i = ranges.first_point_light_index_offset; i < ranges.first_reflection_probe_index_offset; i += 1u) {
        let id = clustering::get_clusterable_object_id(i);
        let light = bindings::clusterable_objects.data[id];
        let offset = light.position_radius.xyz - input.world_position.xyz;
        let distance2 = max(dot(offset, offset), 0.00001);
        var attenuation = getDistanceAttenuation(distance2, light.color_inverse_square_range.w);
        // Clusters overlap light spheres: reject exactly zero range influence
        // before direction normalization, diffuse, cone and shadow evaluation.
        if attenuation <= 0.0 { continue; }
        let direction = offset * inverseSqrt(distance2);
        let diffuse = max(dot(normal, direction), 0.0);
        if attenuation * diffuse <= 0.0 { continue; }
        var shadow = 1.0;
        let shadowed = receives_shadow && (light.flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u;
        if i < ranges.first_spot_light_index_offset {
            if shadowed { shadow = shadows::fetch_point_shadow(id, input.world_position, input.world_normal); }
        } else {
            var axis = vec3(light.light_custom_data.x, 0.0, light.light_custom_data.y);
            axis.y = sqrt(max(0.0, 1.0 - dot(axis, axis)));
            if (light.flags & mesh_view_types::POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u { axis.y = -axis.y; }
            let cone = saturate(dot(-axis, direction) * light.light_custom_data.z + light.light_custom_data.w);
            attenuation *= cone * cone;
            if attenuation <= 0.0 { continue; }
            if shadowed { shadow = shadows::fetch_spot_shadow(id, input.world_position, input.world_normal, light.shadow_map_near_z); }
        }
        irradiance += light.color_inverse_square_range.rgb * (diffuse * attenuation * shadow / PI);
    }
    return vec4(input.material.base_color.rgb * 0.96 * irradiance * bindings::view.exposure,
        input.material.base_color.a);
}
