export type AvailableSkill = {
    name: string;
};

export type SkillSelectionResult<TSkill extends AvailableSkill> = {
    approvedSkills: TSkill[];
    unavailableApprovedSkillNames: string[];
};

export function selectApprovedSkills<TSkill extends AvailableSkill>(
    availableSkills: TSkill[],
    approvedSkillNames: unknown
): SkillSelectionResult<TSkill> {
    if (!Array.isArray(approvedSkillNames)) {
        throw new Error('Agent skills must be an array');
    }

    const invalidSkillNames = approvedSkillNames.filter((skillName) => typeof skillName !== 'string' || skillName.trim().length === 0);
    if (invalidSkillNames.length > 0) {
        throw new Error('Agent skills must contain only non-empty strings');
    }

    if (approvedSkillNames.includes('_all_')) {
        throw new Error('Agent skills does not support _all_; list skill names explicitly');
    }

    const approvedSkills = availableSkills.filter((availableSkill) => approvedSkillNames.includes(availableSkill.name));
    const unavailableApprovedSkillNames = approvedSkillNames
        .filter((skillName: unknown): skillName is string => typeof skillName === 'string')
        .filter((skillName: string) => !approvedSkills.some((approvedSkill) => approvedSkill.name === skillName));

    return { approvedSkills, unavailableApprovedSkillNames };
}
