// Option lists for the passport's about-me dropdowns, keyed by the `ProfileInfo` field they fill.
//
// Ported from the old system scene (bevy-ui-scene's passport-field-selectable-values.json) so a
// profile edited there and one edited here offer the same values — these strings are what actually
// lands in the deployed profile, and unity-explorer matches on them too. The source file's values
// each carried a trailing carriage return (an artifact of however the lists were pasted in); they
// are stripped here, since saving one would write a `Brazil\r` that nothing else matches.
//
// The blank "not set" choice isn't listed — the edit form adds it to every dropdown.

import type { ProfileInfo } from '../../engine/protocol'

export const FIELD_OPTIONS: Partial<Record<keyof ProfileInfo, string[]>> = {
  country: [
    'Afghanistan', 'Albania', 'Algeria', 'Andorra', 'Angola', 'Antigua and Barbuda', 'Argentina', 'Armenia',
    'Australia', 'Austria', 'Azerbaijan', 'Bahamas', 'Bahrain', 'Bangladesh', 'Barbados', 'Belarus', 'Belgium',
    'Belize', 'Benin', 'Bhutan', 'Bolivia', 'Bosnia and Herzegovina', 'Botswana', 'Brazil', 'Brunei', 'Bulgaria',
    'Burkina Faso', 'Burundi', 'Côte d\'Ivoire', 'Cabo Verde', 'Cambodia', 'Cameroon', 'Canada',
    'Central African Republic', 'Chad', 'Chile', 'China', 'Colombia', 'Comoros', 'Congo (Congo-Brazzaville)',
    'Costa Rica', 'Croatia', 'Cuba', 'Cyprus', 'Czechia (Czech Republic)', 'Decentraland',
    'Democratic Republic of the Congo', 'Denmark', 'Djibouti', 'Dominica', 'Dominican Republic', 'Ecuador', 'Egypt',
    'El Salvador', 'Equatorial Guinea', 'Eritrea', 'Estonia', 'Eswatini (fmr. "Swaziland")', 'Ethiopia', 'Fiji',
    'Finland', 'France', 'Gabon', 'Gambia', 'Georgia', 'Germany', 'Ghana', 'Greece', 'Grenada', 'Guatemala',
    'Guinea', 'Guinea-Bissau', 'Guyana', 'Haiti', 'Holy See', 'Honduras', 'Hungary', 'Iceland', 'India',
    'Indonesia', 'Iran', 'Iraq', 'Ireland', 'Israel', 'Italy', 'Jamaica', 'Japan', 'Jordan', 'Kazakhstan', 'Kenya',
    'Kiribati', 'Kuwait', 'Kyrgyzstan', 'Laos', 'Latvia', 'Lebanon', 'Lesotho', 'Liberia', 'Libya', 'Liechtenstein',
    'Lithuania', 'Luxembourg', 'Madagascar', 'Malawi', 'Malaysia', 'Maldives', 'Mali', 'Malta', 'Marshall Islands',
    'Mauritania', 'Mauritius', 'Metaverse', 'Mexico', 'Micronesia', 'Moldova', 'Monaco', 'Mongolia', 'Montenegro',
    'Morocco', 'Mozambique', 'Myanmar (formerly Burma)', 'Namibia', 'Nauru', 'Nepal', 'Netherlands', 'New Zealand',
    'Nicaragua', 'Niger', 'Nigeria', 'North Korea', 'North Macedonia', 'Norway', 'Oman', 'Pakistan', 'Palau',
    'Palestine State', 'Panama', 'Papua New Guinea', 'Paraguay', 'Peru', 'Philippines', 'Poland', 'Portugal',
    'Qatar', 'Romania', 'Russia', 'Rwanda', 'Saint Kitts and Nevis', 'Saint Lucia',
    'Saint Vincent and the Grenadines', 'Samoa', 'San Marino', 'Sao Tome and Principe', 'Saudi Arabia', 'Senegal',
    'Serbia', 'Seychelles', 'Sierra Leone', 'Singapore', 'Slovakia', 'Slovenia', 'Solomon Islands', 'Somalia',
    'South Africa', 'South Korea', 'South Sudan', 'Spain', 'Sri Lanka', 'Sudan', 'Suriname', 'Sweden',
    'Switzerland', 'Syria', 'Tajikistan', 'Tanzania', 'Thailand', 'Timor-Leste', 'Togo', 'Tonga',
    'Trinidad and Tobago', 'Tunisia', 'Turkey', 'Turkmenistan', 'Tuvalu', 'Uganda', 'Ukraine',
    'United Arab Emirates', 'United Kingdom', 'United States of America', 'Uruguay', 'Uzbekistan', 'Vanuatu',
    'Venezuela', 'Vietnam', 'Yemen', 'Zambia', 'Zimbabwe', 'Other'
  ],
  language: [
    'Afrikanns', 'Albanian', 'Arabic', 'Armenian', 'Basque', 'Bengali', 'Bulgarian', 'Catalan', 'Cambodian',
    'Chinese (Mandarin)', 'Croation', 'Czech', 'Danish', 'Dutch', 'English', 'Estonian', 'Fiji', 'Finnish',
    'French', 'Georgian', 'German', 'Greek', 'Gujarati', 'Hebrew', 'Hindi', 'Hungarian', 'Icelandic', 'Indonesian',
    'Irish', 'Italian', 'Japanese', 'Javanese', 'Korean', 'Latin', 'Latvian', 'Lithuanian', 'Macedonian', 'Malay',
    'Malayalam', 'Maltese', 'Maori', 'Marathi', 'Mongolian', 'Nepali', 'Norwegian', 'Persian', 'Polish',
    'Portuguese', 'Punjabi', 'Quechua', 'Romanian', 'Russian', 'Samoan', 'Serbian', 'Slovak', 'Slovenian',
    'Spanish', 'Swahili', 'Swedish', 'Tamil', 'Tatar', 'Telugu', 'Thai', 'Tibetan', 'Tonga', 'Turkish', 'Ukrainian',
    'Urdu', 'Uzbek', 'Vietnamese', 'Welsh', 'Xhosa', 'Other'
  ],
  gender: [
    'Female', 'Male', 'Transgender', 'Neutral', 'Non-Binary', 'Fluid', 'Agender', 'Pangender', 'Queer',
    'Two-Spirit', 'Other'
  ],
  pronouns: ['He / Him', 'She / Her', 'They / Them', 'Ze / Zir', 'Xe / Xem', 'Ze / Hir', 'Per / Per', 'Other'],
  relationship: [
    'Single', 'In a relationship', 'Engaged', 'Married', 'It\'s complicated', 'In an open relationship', 'Widowed',
    'Separated', 'Divorced', 'Other'
  ],
  sexualOrientation: [
    'Heterosexual', 'Bisexual', 'Lesbian', 'Gay', 'Asexual', 'Queer', 'Polysexual', 'Pansexual', 'Other'
  ],
  employment: ['Studying', 'Working', 'Chilling', 'Looking for job', 'Home/Family', 'Retired', 'Other'],
}
